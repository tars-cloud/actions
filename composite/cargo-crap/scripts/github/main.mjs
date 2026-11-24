import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, readFileSync } from "node:fs";
import { join, posix } from "node:path";

const input = (name, fallback = "") => process.env[`INPUT_${name.toUpperCase()}`] ?? fallback;
const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"));
const repo = process.env.GITHUB_REPOSITORY;
const api = process.env.GITHUB_API_URL ?? "https://api.github.com";
const marker = "<!-- tars-cloud/actions:cargo-crap:v1 -->";
const commitTrailer = "Tars-Cloud-CRAP: v2";
const branch = input("baseline-branch") || event.repository.default_branch;
const identity = input("analysis-id", "default");
if (!/^[a-zA-Z0-9_-]{1,64}$/.test(identity))
  throw new Error("analysis-id must use 1-64 letters, digits, underscores or hyphens");
const recordFiles =
  identity === "default"
    ? ["baseline.json", "crap-badge.json"]
    : [`${identity}/baseline.json`, `${identity}/crap-badge.json`];
const allowedRecordPath = /^(?:[a-zA-Z0-9_-]{1,64}\/)?(?:baseline\.json|crap-badge\.json)$/;
const artifactName = `cargo-crap-${identity}`;
const sha = (value) => {
  if (!/^[a-f0-9]{40}$/.test(value ?? "")) throw new Error("Expected a full commit SHA");
  return value;
};
const out = (name, value) => {
  if (String(value).includes("\n") || String(value).includes("\r")) throw new Error("Invalid output");
  appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
};
function managedBranch() {
  const managed = input("records-branch", "crap");
  if (managed !== "crap" && !/^tact-crap-records-[0-9]+-[0-9]+$/.test(managed))
    throw new Error("Invalid managed recording branch");
  return managed;
}
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
const hasRecordMarker = (message) => Boolean(message?.split("\n").some((line) => line === commitTrailer));
const git = (cwd, args) =>
  execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null" },
    maxBuffer: 16 * 1024 * 1024,
  });
const gitBlob = (cwd, revision, filePath) => {
  try {
    return git(cwd, ["show", `${revision}:${filePath}`]);
  } catch {
    return null;
  }
};
function normalizeCargoLock(contents) {
  const blocks = contents.split(/(?=^\[\[package\]\]\s*$)/m);
  const local = new Set();
  const firstPass = blocks.map((block) => {
    if (!block.startsWith("[[package]]") || /^source\s*=/m.test(block)) return block;
    const name = block.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    const version = block.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    if (name && version) local.add(`${name} ${version}`);
    return block.replace(/^version\s*=\s*"[^"]+"/m, 'version = "0.0.0"');
  });
  return firstPass
    .map((block) =>
      block.replace(/"([^"\s]+) ([^"\s]+)"/g, (full, name, version) =>
        local.has(`${name} ${version}`) ? `"${name} 0.0.0"` : full,
      ),
    )
    .join("");
}
function normalizeCargoManifest(contents) {
  return contents
    .replace(/^\s*version\s*=\s*"[^"]+"\s*$/gm, 'version = "0.0.0"')
    .replace(/(path\s*=\s*"[^"]*"[^}\n]*?\bversion\s*=\s*")([=~^]?)[^"]+(")/g, "$1$20.0.0$3")
    .replace(/(\bversion\s*=\s*")([=~^]?)[^"]+("[^}\n]*?\bpath\s*=\s*"[^"]*")/g, "$1$20.0.0$3");
}
function isRelevantPath(filePath, relativeDir, crates) {
  const base = posix.basename(filePath);
  const ext = posix.extname(base).slice(1).toLowerCase();
  if (["rs", "nix", "c", "cc", "cpp", "cxx", "h", "hh", "hpp", "s", "proto", "wit"].includes(ext)) return true;
  if (
    [
      "devenv.yaml",
      "devenv.lock",
      "flake.lock",
      "rust-toolchain",
      "rust-toolchain.toml",
      "secretspec.toml",
      ".cargo-crap.toml",
      "build.rs",
    ].includes(base)
  )
    return true;
  const segments = filePath.split("/");
  if (segments.includes(".cargo") || filePath.startsWith(".github/workflows/") || filePath.startsWith("composite/"))
    return true;
  if ([...crates].some((dir) => filePath.startsWith(`${dir}/`))) return true;
  const scoped =
    relativeDir && filePath.startsWith(`${relativeDir}/`) ? filePath.slice(relativeDir.length + 1) : filePath;
  return ["src/", "tests/", "benches/", "examples/", "testdata/", "fixtures/"].some(
    (prefix) => filePath.startsWith(prefix) || scoped.startsWith(prefix),
  );
}
function eligibleAncestors(workspace, rawWorkingDir, target) {
  if (!workspace) return [];
  const relativeDir = posix
    .normalize(rawWorkingDir || ".")
    .replace(/^\.\/?/, "")
    .replace(/\/$/, "");
  let revisions;
  try {
    revisions = git(workspace, ["rev-list", "--first-parent", "--max-count=21", target])
      .trim()
      .split("\n")
      .filter((entry) => /^[a-f0-9]{40}$/.test(entry));
  } catch {
    return [];
  }
  const eligible = [];
  for (const candidate of revisions.slice(1)) {
    const changed = git(workspace, ["diff", "--name-only", "-z", candidate, target]).split("\0").filter(Boolean);
    const crates = new Set();
    for (const rev of [candidate, target]) {
      for (const entry of git(workspace, ["ls-tree", "-r", "--name-only", "-z", rev]).split("\0").filter(Boolean)) {
        if (posix.basename(entry) === "Cargo.toml") {
          const dir = posix.dirname(entry);
          if (dir !== "." && dir !== relativeDir) crates.add(dir);
        }
      }
    }
    const affects = changed.some((filePath) => {
      const base = posix.basename(filePath);
      if (base === "Cargo.toml") {
        const left = gitBlob(workspace, candidate, filePath);
        const right = gitBlob(workspace, target, filePath);
        return left === null || right === null || normalizeCargoManifest(left) !== normalizeCargoManifest(right);
      }
      if (base === "Cargo.lock") {
        const left = gitBlob(workspace, candidate, filePath);
        const right = gitBlob(workspace, target, filePath);
        return left === null || right === null || normalizeCargoLock(left) !== normalizeCargoLock(right);
      }
      return isRelevantPath(filePath, relativeDir, crates);
    });
    if (affects) break;
    eligible.push(candidate);
  }
  return eligible;
}
async function resolve() {
  if (!branch || branch.startsWith("-") || /[\r\n]/.test(branch)) throw new Error("Invalid baseline branch");
  const managed = managedBranch();
  const commit = sha(process.env.GITHUB_SHA);
  let operation;
  let baselineCommit = "";
  let headCommit = "";
  if (process.env.GITHUB_EVENT_NAME === "pull_request") {
    const pr = event.pull_request;
    if (pr.base.ref !== branch) throw new Error("PR must target the designated baseline branch");
    const merge = await request(`git/commits/${commit}`);
    if (merge.parents.length !== 2 || merge.parents[1].sha !== pr.head.sha)
      throw new Error(
        "Proposed merge must have two parents and match the PR event head; synchronize the branch and rerun",
      );
    operation = "compare";
    // The event's base SHA can predate the base used for GitHub's captured merge commit.
    baselineCommit = sha(merge.parents[0].sha);
    headCommit = sha(pr.head.sha);
  } else if (process.env.GITHUB_EVENT_NAME === "merge_group") {
    const group = event.merge_group;
    if (group.base_ref !== `refs/heads/${branch}`)
      throw new Error("Merge group must target the designated baseline branch");
    if (sha(group.head_sha) !== commit) throw new Error("Merge group head does not match the workflow SHA");
    const base = sha(group.base_sha);
    const comparison = await request(`compare/${base}...${commit}?per_page=1`);
    if (comparison.merge_base_commit?.sha !== base) throw new Error("Merge group base must be an ancestor of its head");
    operation = "compare";
    baselineCommit = base;
  } else if (
    ["push", "workflow_dispatch"].includes(process.env.GITHUB_EVENT_NAME) &&
    process.env.GITHUB_REF === `refs/heads/${branch}`
  ) {
    operation = "measure";
  } else {
    throw new Error(
      "Cargo CRAP supports pull_request or merge_group to the baseline branch, or push/workflow_dispatch on that branch",
    );
  }
  const existingBaseline = await request(
    `contents/${recordFiles[0]}?ref=${encodeURIComponent(managed)}`,
    "GET",
    undefined,
    true,
    "application/vnd.github.object+json",
  );
  out("baseline-branch", branch);
  out("commit", commit);
  out("bootstrapped", existingBaseline?.type === "file" ? "true" : "false");
  out("operation", operation);
  out("baseline-commit", baselineCommit);
  out("head-commit", headCommit);
}
async function findArtifactForCommit(runs, candidate, excludeRunId = 0) {
  for (const run of runs) {
    if (
      run.id === excludeRunId ||
      run.head_sha !== candidate ||
      run.head_branch !== branch ||
      !["push", "workflow_dispatch"].includes(run.event) ||
      run.repository.full_name !== repo
    )
      continue;
    const artifacts = await pages(`actions/runs/${run.id}/artifacts`, "artifacts");
    const artifact = artifacts.find((a) => a.name === artifactName && !a.expired);
    if (artifact) return { artifactId: artifact.id, runId: run.id };
  }
  return null;
}
async function findBaseline() {
  const rawBaseline = input("baseline-commit");
  if (!rawBaseline && process.env.GITHUB_EVENT_NAME !== "push") sha(rawBaseline);
  const workspace = process.env.GITHUB_WORKSPACE ?? "";
  const workingDir = input("working-directory", ".");
  const currentRunId = Number(process.env.GITHUB_RUN_ID);
  if (rawBaseline) {
    const commit = sha(rawBaseline);
    const current = await request(`actions/runs/${process.env.GITHUB_RUN_ID}`);
    const exactRuns = await pages(
      `actions/workflows/${current.workflow_id}/runs?branch=${encodeURIComponent(branch)}&head_sha=${commit}&status=completed`,
      "workflow_runs",
    );
    const exact = await findArtifactForCommit(exactRuns, commit);
    if (exact) {
      out("artifact-id", exact.artifactId);
      out("run-id", exact.runId);
      return;
    }
    const ancestors = eligibleAncestors(workspace, workingDir, commit);
    if (ancestors.length > 0) {
      const recent = await request(
        `actions/workflows/${current.workflow_id}/runs?branch=${encodeURIComponent(branch)}&status=completed&per_page=100`,
      );
      for (const candidate of ancestors) {
        const hit = await findArtifactForCommit(recent.workflow_runs ?? [], candidate, currentRunId);
        if (hit) {
          out("artifact-id", hit.artifactId);
          out("run-id", hit.runId);
          return;
        }
      }
    }
  } else if (workspace) {
    const commit = sha(process.env.GITHUB_SHA);
    const ancestors = eligibleAncestors(workspace, workingDir, commit);
    if (ancestors.length > 0) {
      const current = await request(`actions/runs/${process.env.GITHUB_RUN_ID}`);
      const recent = await request(
        `actions/workflows/${current.workflow_id}/runs?branch=${encodeURIComponent(branch)}&status=completed&per_page=100`,
      );
      for (const candidate of ancestors) {
        const hit = await findArtifactForCommit(recent.workflow_runs ?? [], candidate, currentRunId);
        if (hit) {
          out("artifact-id", hit.artifactId);
          out("run-id", hit.runId);
          return;
        }
      }
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
function recordOwner(head) {
  if (!hasRecordMarker(head.commit?.message)) return;
  if (head.committer?.type === "Bot") return head.committer.login;
  // GitHub signs App-created and GITHUB_TOKEN commits as web-flow with a Bot author.
  const verification = head.commit.verification;
  if (
    head.committer?.login === "web-flow" &&
    verification?.verified === true &&
    verification.reason === "valid" &&
    head.author?.type === "Bot"
  )
    return head.author.login;
}
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
    out("records-branch", "");
    out("records-commit", "");
    return;
  }
  const managed = managedBranch();
  const baseline = readFileSync(join(directory, "baseline.json"));
  const badge = readFileSync(join(directory, "crap-badge.json"));
  if (hash(baseline) !== metadata.baseline_hash || hash(badge) !== metadata.badge_hash)
    throw new Error("Generated record hash mismatch");
  const contents = [baseline.toString(), badge.toString()];
  const old = await request(refPath(managed), "GET", undefined, true);
  let oldTreeSha = null;
  const existingBlobs = new Map();
  if (old) {
    const head = await request(`commits/${sha(old.object.sha)}`);
    if (!recordOwner(head)) throw new Error(`Refusing to overwrite an unowned ${managed} branch`);
    oldTreeSha = head.commit?.tree?.sha ?? head.tree?.sha;
    if (!oldTreeSha) throw new Error("Missing managed branch tree SHA");
    const oldTree = await request(`git/trees/${oldTreeSha}?recursive=1`);
    if (oldTree.truncated) throw new Error(`Managed branch ${managed} tree is unexpectedly truncated`);
    for (const entry of oldTree.tree ?? []) {
      if (entry.type === "tree") continue;
      if (entry.type !== "blob" || entry.mode !== "100644" || !allowedRecordPath.test(entry.path))
        throw new Error(`Managed branch ${managed} contains unexpected entry: ${entry.path}`);
      existingBlobs.set(entry.path, entry.sha);
    }
    if (recordFiles.every((filePath, i) => existingBlobs.get(filePath) === blobHash(contents[i]))) {
      console.log(`Recorded scores on ${managed} already match; no commit needed.`);
      out("records-branch", managed);
      out("records-commit", "");
      return;
    }
  }
  const treePayload = {
    tree: recordFiles.map((filePath, i) => ({
      path: filePath,
      mode: "100644",
      type: "blob",
      content: contents[i],
    })),
  };
  if (oldTreeSha) treePayload.base_tree = oldTreeSha;
  const tree = await request("git/trees", "POST", treePayload);
  // Keep updates fast-forward to the observed managed head, so a concurrent update is rejected.
  const parents = old ? [sha(old.object.sha)] : [];
  const commit = await request("git/commits", "POST", {
    message: `chore(crap): record ${identity} baseline and badge for ${revision.slice(0, 12)}\n\nSource-Commit: ${revision}\nAnalysis-ID: ${identity}\n${commitTrailer}`,
    tree: tree.sha,
    parents,
  });
  if ((await request(refPath(branch))).object.sha !== revision) {
    console.log("Baseline branch advanced; publication skipped.");
    out("records-branch", "");
    out("records-commit", "");
    return;
  }
  if (old) await request(`git/refs/heads/${managed}`, "PATCH", { sha: commit.sha, force: false });
  else await request("git/refs", "POST", { ref: `refs/heads/${managed}`, sha: commit.sha });
  out("records-branch", managed);
  out("records-commit", commit.sha);
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
