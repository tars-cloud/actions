import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createHash } from "node:crypto";

const input = (name, fallback = "") => process.env[`INPUT_${name.toUpperCase()}`] ?? fallback;
const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"));
const repo = process.env.GITHUB_REPOSITORY;
const api = process.env.GITHUB_API_URL ?? "https://api.github.com";
const marker = "<!-- tars-cloud/actions:cargo-crap:v1 -->";
const branch = input("baseline-branch") || event.repository.default_branch;
const identity = input("analysis-id", "default");
if (!/^[a-zA-Z0-9_-]{1,64}$/.test(identity))
  throw new Error("analysis-id must use 1-64 letters, digits, underscores or hyphens");
const artifactName = `cargo-crap-${identity}`;
const sha = (value) => {
  if (!/^[a-f0-9]{40}$/.test(value ?? "")) throw new Error("Expected a full commit SHA");
  return value;
};
const out = (name, value) => {
  if (String(value).includes("\n") || String(value).includes("\r")) throw new Error("Invalid output");
  appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
};
async function request(path, method = "GET", body, missing = false, media = "application/vnd.github+json") {
  const response = await fetch(`${api}/repos/${repo}/${path}`, {
    method,
    headers: {
      Authorization: `Bearer ${input("token")}`,
      Accept: media,
      "X-GitHub-Api-Version": "2022-11-28",
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (missing && response.status === 404) return null;
  if (!response.ok) throw new Error(`GitHub API ${method} ${path}: ${response.status}`);
  return response.status === 204 ? null : response.json();
}
async function pages(path, property) {
  const values = [];
  for (let page = 1; ; page++) {
    const response = await request(`${path}${path.includes("?") ? "&" : "?"}per_page=100&page=${page}`);
    const entries = property ? response[property] : response;
    values.push(...entries);
    if (entries.length < 100) return values;
  }
}
const refPath = (name) => `git/ref/heads/${encodeURIComponent(name)}`;
async function resolve() {
  if (!branch || branch.startsWith("-") || /[\r\n]/.test(branch)) throw new Error("Invalid baseline branch");
  const commit = sha(process.env.GITHUB_SHA);
  out("baseline-branch", branch);
  out("commit", commit);
  if (process.env.GITHUB_EVENT_NAME === "pull_request") {
    const pr = event.pull_request;
    if (pr.base.ref !== branch) throw new Error("PR must target the designated baseline branch");
    const merge = await request(`git/commits/${commit}`);
    if (merge.parents.length !== 2 || merge.parents[1].sha !== pr.head.sha || merge.parents[0].sha !== pr.base.sha)
      throw new Error("Proposed merge parents do not match the PR event; synchronize the branch and rerun");
    out("operation", "compare");
    out("baseline-commit", sha(merge.parents[0].sha));
    out("head-commit", sha(pr.head.sha));
  } else if (
    ["push", "workflow_dispatch"].includes(process.env.GITHUB_EVENT_NAME) &&
    process.env.GITHUB_REF === `refs/heads/${branch}`
  ) {
    out("operation", "measure");
    out("baseline-commit", "");
    out("head-commit", "");
  } else {
    throw new Error(
      "Cargo CRAP supports pull_request to the baseline branch, or push/workflow_dispatch on that branch",
    );
  }
}
async function findBaseline() {
  const commit = sha(input("baseline-commit"));
  const current = await request(`actions/runs/${process.env.GITHUB_RUN_ID}`);
  const runs = await pages(
    `actions/workflows/${current.workflow_id}/runs?branch=${encodeURIComponent(branch)}&head_sha=${commit}&status=completed`,
    "workflow_runs",
  );
  for (const run of runs) {
    if (
      run.head_sha !== commit ||
      run.head_branch !== branch ||
      !["push", "workflow_dispatch"].includes(run.event) ||
      run.repository.full_name !== repo
    )
      continue;
    const artifacts = await pages(`actions/runs/${run.id}/artifacts`, "artifacts");
    const artifact = artifacts.find((a) => a.name === artifactName && !a.expired);
    if (artifact) {
      out("artifact-id", artifact.id);
      out("run-id", run.id);
      return;
    }
  }
  out("artifact-id", "");
  out("run-id", "");
}
function report() {
  const directory = input("report-directory");
  const metadata = JSON.parse(readFileSync(join(directory, "metadata.json"), "utf8"));
  if (
    metadata.complete !== true ||
    metadata.run_id !== process.env.GITHUB_RUN_ID ||
    metadata.commit !== process.env.GITHUB_SHA
  )
    throw new Error("Report provenance does not match this run");
  return { directory, metadata };
}
async function comment() {
  const { directory, metadata } = report();
  if (metadata.operation !== "compare" || process.env.GITHUB_EVENT_NAME !== "pull_request")
    throw new Error("Comments require PR analysis");
  const pr = await request(`pulls/${event.number}`);
  if (pr.state !== "open" || pr.head.sha !== metadata.head_commit || pr.base.sha !== metadata.baseline_commit) {
    console.log("Superseded PR analysis; comment skipped.");
    return;
  }
  const ownership = `${marker}\n<!-- analysis:${identity} -->`;
  const summary = readFileSync(join(directory, "summary.md"), "utf8");
  const body = `${ownership}\n\n${summary}\n[Download the reports](${process.env.GITHUB_SERVER_URL}/${repo}/actions/runs/${process.env.GITHUB_RUN_ID}).\n`;
  const comments = await pages(`issues/${event.number}/comments`);
  const existing = comments.find(
    (c) => c.user.login === "github-actions[bot]" && c.user.type === "Bot" && c.body.startsWith(ownership),
  );
  if (existing) await request(`issues/comments/${existing.id}`, "PATCH", { body });
  else await request(`issues/${event.number}/comments`, "POST", { body });
}
const hash = (content) => createHash("sha256").update(content).digest("hex");
const blobHash = (content) =>
  createHash("sha1")
    .update(`blob ${Buffer.byteLength(content)}\0`)
    .update(content)
    .digest("hex");
async function record() {
  const { directory, metadata } = report();
  if (
    metadata.operation !== "measure" ||
    !["push", "workflow_dispatch"].includes(process.env.GITHUB_EVENT_NAME) ||
    process.env.GITHUB_REF !== `refs/heads/${branch}`
  )
    throw new Error("Recording requires a trusted baseline branch run");
  const revision = sha(metadata.commit);
  const tip = await request(refPath(branch));
  if (tip.object.sha !== revision) {
    console.log("Baseline branch advanced; stale publication skipped.");
    return;
  }
  const managed = input("records-branch", "crap/next");
  if (managed !== "crap/next" && !/^tact-crap-records-[0-9]+-[0-9]+$/.test(managed))
    throw new Error("Invalid managed recording branch");
  const pulls = await pages(`pulls?state=open&head=${encodeURIComponent(repo.split("/")[0] + ":" + managed)}`);
  if (pulls.length > 1) throw new Error("More than one managed recording PR");
  const existing = pulls[0];
  const old = await request(refPath(managed), "GET", undefined, true);
  const allowed = [".github/crap/baseline.json", ".github/badges/crap-badge.json"];
  if (existing) {
    if (
      existing.base.ref !== branch ||
      existing.head.repo.full_name !== repo ||
      existing.user.type !== "Bot" ||
      !existing.body?.startsWith(marker)
    )
      throw new Error("Refusing to modify an unowned crap/next PR");
    const files = await pages(`pulls/${existing.number}/files`);
    if (files.some((f) => !allowed.includes(f.filename))) throw new Error("Managed PR contains unrelated files");
    const head = await request(`commits/${existing.head.sha}`);
    if (
      !head.commit.message.includes(marker) ||
      head.committer?.type !== "Bot" ||
      head.committer.login !== existing.user.login
    )
      throw new Error("Managed branch has an unowned head commit");
  } else if (old) {
    const head = await request(`commits/${old.object.sha}`);
    if (!head.commit.message.includes(marker) || head.committer?.type !== "Bot")
      throw new Error("Refusing to overwrite an unowned crap/next branch");
  }
  const baseline = readFileSync(join(directory, "baseline.json"));
  const badge = readFileSync(join(directory, "crap-badge.json"));
  if (hash(baseline) !== metadata.baseline_hash || hash(badge) !== metadata.badge_hash)
    throw new Error("Generated record hash mismatch");
  const contents = [baseline.toString(), badge.toString()];
  const unchanged = await Promise.all(
    allowed.map(async (path, i) => {
      const current = await request(
        `contents/${path}?ref=${revision}`,
        "GET",
        undefined,
        true,
        "application/vnd.github.object+json",
      );
      return current?.type === "file" && current.sha === blobHash(contents[i]);
    }),
  );
  if (unchanged.every(Boolean)) {
    if (existing) await request(`pulls/${existing.number}`, "PATCH", { state: "closed" });
    console.log("Recorded scores already match; no recording PR needed.");
    return;
  }
  const trunk = await request(`git/commits/${revision}`);
  const tree = await request("git/trees", "POST", {
    base_tree: trunk.tree.sha,
    tree: allowed.map((path, i) => ({ path, mode: "100644", type: "blob", content: contents[i] })),
  });
  // Keep updates fast-forward to the observed managed head, so a concurrent update is rejected.
  const parents = [...new Set([revision, ...(old ? [old.object.sha] : [])])];
  const commit = await request("git/commits", "POST", {
    message: `chore(crap): record baseline and badge\n\n${marker}`,
    tree: tree.sha,
    parents,
  });
  if ((await request(refPath(branch))).object.sha !== revision) {
    console.log("Baseline branch advanced; publication skipped.");
    return;
  }
  if (old) await request(`git/refs/heads/${managed}`, "PATCH", { sha: commit.sha, force: false });
  else await request("git/refs", "POST", { ref: `refs/heads/${managed}`, sha: commit.sha });
  const body = `${marker}\n\nRecord the CRAP baseline and badge measured at \`${revision}\`.\n\n[Measurement run](${process.env.GITHUB_SERVER_URL}/${repo}/actions/runs/${process.env.GITHUB_RUN_ID}).\n\nMerging publishes the reviewed JSON files.\nWork PRs continue comparing against the actual baseline branch while this PR waits.\n`;
  const fields = { title: "Update CRAP Baseline and Badge", body, base: branch };
  const pr = existing
    ? await request(`pulls/${existing.number}`, "PATCH", fields)
    : await request("pulls", "POST", { ...fields, head: managed });
  out("pr-url", pr.html_url);
}
try {
  const phase = input("phase");
  const operations = { resolve, "find-baseline": findBaseline, comment, record };
  if (!operations[phase]) throw new Error("Unknown Cargo CRAP GitHub phase");
  await operations[phase]();
} catch (error) {
  console.error(`::error::${error.message}`);
  process.exitCode = 1;
}
