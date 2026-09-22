const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { cachePlan, policy } = require("../internal/cache-plan/main.cjs");

const scratch = path.resolve(".tars/scratch/tests");
fs.mkdirSync(scratch, { recursive: true });
const s3 = {
  "s3-endpoint": "https://cache.example.invalid",
  "s3-bucket": "fixture",
  "s3-region": "us-east-1",
  "s3-access-key": "fixture-key",
  "s3-secret-key": "fixture-secret",
};
const context = {
  runner: "self-hosted",
  os: "Linux",
  arch: "X64",
  repository: "example/project",
  defaultBranch: "trunk",
  ref: "refs/heads/topic",
};
function fixture(t, files = {}) {
  const root = fs.mkdtempSync(path.join(scratch, "cache-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const write = (name, content) => {
    fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true });
    fs.writeFileSync(path.join(root, name), content);
  };
  for (const [name, content] of Object.entries({
    "devenv.nix": "{}",
    "devenv.yaml": "inputs: {}",
    "devenv.lock": "{}",
    ...files,
  }))
    write(name, content);
  return {
    root,
    write,
    plan: (config = {}, changes = {}, env = {}) =>
      cachePlan(
        config,
        { ...context, workspace: root, ...changes },
        { HOME: path.join(root, "home"), ...env },
        new Date("2026-09-22T12:00:00Z"),
      ),
  };
}

test("backend routing covers hosted, unconfigured self-hosted, complete and partial S3", () => {
  assert.equal(policy({}, context).backend, "github");
  assert.equal(policy(s3, context).backend, "s3");
  assert.equal(policy({ "s3-bucket": "unused" }, { ...context, runner: "github-hosted" }).backend, "github");
  assert.throws(
    () => policy({ "s3-bucket": "fixture" }, context),
    /s3-endpoint, s3-region, s3-access-key, s3-secret-key/,
  );
  assert.throws(
    () => policy({ "s3-session-token": "secret-value" }, context),
    (error) => !error.message.includes("secret-value"),
  );
});

test("fork policy precedes S3 validation and disables Cachix writes for any PR-bearing event", () => {
  for (const runner of ["self-hosted", "github-hosted"]) {
    const result = policy(
      { "s3-bucket": "unused", "cachix-name": "public", "cachix-token": "secret" },
      { ...context, runner, headRepository: "fork/project", pr: 7 },
    );
    assert.equal(result.backend, "github");
    assert.equal(result.cachix, "read");
    assert.equal(result.fork, true);
  }
  assert.equal(
    policy(
      { ...s3, "cachix-name": "public", "cachix-token": "secret" },
      { ...context, headRepository: context.repository, pr: 7 },
    ).cachix,
    "write",
  );
  assert.equal(policy({ "cachix-token": "secret" }, context).cachix, "disabled");
  assert.equal(policy({ "cachix-name": "public" }, context).cachix, "read");
});

test("reject unsupported platforms before discovery", () => {
  for (const changes of [{ os: "Windows" }, { os: "macOS" }, { arch: "ARM" }, { runner: "unknown" }]) {
    assert.throws(() => cachePlan({}, { ...context, ...changes, workspace: "/missing" }), /platforms|runner/);
  }
  assert.equal(policy({}, { ...context, arch: "ARM64" }).backend, "github");
});

test("nested mixed detection excludes installations, scratch, workflow Trivy names and symlinks", (t) => {
  const f = fixture(t, {
    "rust/Cargo.toml": "[package]",
    "python/uv.lock": "uv",
    "python/pyproject.toml": "[project]",
    "legacy/requirements.txt": "example==1",
    "frontend/bun.lockb": "binary",
    "security/trivy.yaml": "{}",
    "scratch/ignored/bun.lock": "ignored",
    "node_modules/fake/Cargo.toml": "ignored",
  });
  fs.symlinkSync("/tmp", path.join(f.root, "outside"));
  assert.deepEqual(f.plan().tools, ["cargo", "bun", "trivy", "uv", "pip"]);
  assert(!f.plan().caches.cargo.path.includes("/target"));
  assert(!f.plan().caches.cargo.path.includes("/registry/src"));
  const onlyWorkflow = fixture(t, { ".github/workflows/trivy.yml": "{}", "nested/.github/workflows/trivy.yaml": "{}" });
  assert.deepEqual(onlyWorkflow.plan().tools, []);
  assert.deepEqual(f.plan({ exclude: "rust\nfrontend\npython\nlegacy\nsecurity" }).tools, []);
});

test("ambiguous manifests produce actionable notices and explicit overrides resolve them", (t) => {
  const f = fixture(t, { "package.json": "{}", "pyproject.toml": "[project]" });
  assert.deepEqual(f.plan().tools, []);
  assert.equal(f.plan().reasons.length, 2);
  assert.deepEqual(f.plan({ tools: "bun python trivy", "python-manager": "uv" }).tools, ["bun", "trivy", "uv"]);
  f.write("package.json", '{"packageManager":"bun@1.3.0"}');
  assert.deepEqual(f.plan().tools, ["bun"]);
  assert.deepEqual(f.plan({ tools: "none" }).tools, []);
  assert.deepEqual(f.plan({ tools: "none", "trivy-cache-path": "/" }).tools, []);
  assert.throws(() => f.plan({ tools: "node" }), /tools/);
  assert.throws(() => f.plan({ "python-manager": "poetry" }), /python-manager/);
});

test("custom paths take input, job environment and Linux default precedence", (t) => {
  const f = fixture(t);
  const config = {
    tools: "cargo python bun trivy",
    "python-manager": "uv",
    "cargo-target": "true",
    "cargo-cache-path": "custom/cargo",
    "uv-cache-path": "custom/uv",
  };
  const p = f.plan(
    config,
    {},
    {
      CARGO_HOME: "/unused",
      UV_CACHE_DIR: "/unused",
      BUN_INSTALL_CACHE_DIR: "/bun-cache",
      XDG_CACHE_HOME: "/xdg",
      CARGO_TARGET_DIR: "out",
    },
  );
  assert.equal(
    p.caches.cargo.path,
    ["registry/index", "registry/cache", "git/db"].map((x) => path.join(f.root, "custom/cargo", x)).join("\n"),
  );
  assert.equal(p.caches.uv.path, path.join(f.root, "custom/uv"));
  assert.equal(p.caches.bun.path, "/bun-cache");
  assert.equal(p.caches.trivy.path, "/xdg/trivy");
  assert.equal(p.caches["cargo-target"].path, path.join(f.root, "out"));
  assert.equal(p.exports.CARGO_HOME, path.join(f.root, "custom/cargo"));
  assert.throws(() => f.plan({ tools: "trivy", "trivy-cache-path": "/\nINJECT=yes" }), /literal directory/);
  assert.throws(() => f.plan({ tools: "trivy", "trivy-cache-path": "/" }), /dedicated/);
});

test("tool changes invalidate only their own dependency keys; absent tool locks work", (t) => {
  const f = fixture(t, { "Cargo.toml": "[package]", "bun.lock": "a" });
  const before = f.plan();
  f.write("bun.lock", "b");
  assert.equal(f.plan().caches.cargo.key, before.caches.cargo.key);
  assert.notEqual(f.plan().caches.bun.key, before.caches.bun.key);
  f.write("scratch/Cargo.lock", "irrelevant");
  assert.equal(f.plan().caches.cargo.key, before.caches.cargo.key);
  f.write("Cargo.lock", "added");
  assert.notEqual(f.plan().caches.cargo.key, before.caches.cargo.key);
});

test("environment type, lock, shell and architecture isolate restore compatibility", (t) => {
  const f = fixture(t, { "Cargo.toml": "[package]", "flake.nix": "{}", "flake.lock": "{}" });
  const direct = f.plan().caches.cargo;
  const flakes = f.plan({ type: "flakes" }).caches.cargo;
  const named = f.plan({ type: "flakes", "flake-shell": ".#named" }).caches.cargo;
  assert.notEqual(direct.restore, flakes.restore);
  assert.notEqual(named.restore, flakes.restore);
  assert.notEqual(f.plan({}, { arch: "ARM64" }).caches.cargo.restore, direct.restore);
  f.write("devenv.lock", '{"changed":true}');
  assert.notEqual(f.plan().caches.cargo.restore, direct.restore);
  fs.unlinkSync(path.join(f.root, "devenv.yaml"));
  assert.equal(f.plan({ type: "flakes" }).caches.cargo.key, flakes.key);
  assert.throws(() => f.plan(), /devenv.yaml/);
});

test("compiled cache separates target, variant and compiler files from download keys", (t) => {
  const f = fixture(t, { "Cargo.toml": "[package]", "rust-toolchain.toml": '[toolchain]\nchannel="stable"' });
  const cfg = { "cargo-target": "true" };
  const before = f.plan(cfg);
  for (const variant of [
    { "cargo-build-variant": "release-feature-a" },
    { "cargo-build-target": "aarch64-unknown-linux-gnu" },
  ]) {
    const changed = f.plan({ ...cfg, ...variant });
    assert.notEqual(changed.caches["cargo-target"].restore, before.caches["cargo-target"].restore);
    assert.equal(changed.caches.cargo.key, before.caches.cargo.key);
  }
  f.write("rust-toolchain.toml", '[toolchain]\nchannel="nightly"');
  assert.notEqual(f.plan(cfg).caches["cargo-target"].restore, before.caches["cargo-target"].restore);
  assert.equal(f.plan(cfg).caches.cargo.key, before.caches.cargo.key);
  const toolchainOnly = f.plan(cfg).caches["cargo-target"].restore;
  f.write(".cargo/config.toml", '[build]\ntarget="aarch64-unknown-linux-gnu"');
  assert.notEqual(f.plan(cfg).caches["cargo-target"].restore, toolchainOnly);
  assert.equal(f.plan(cfg).caches.cargo.key, before.caches.cargo.key);
  f.write(".cargo/registry/Cargo.toml", "[package]");
  assert.equal(f.plan(cfg).caches.cargo.key, before.caches.cargo.key);
});

test("PR cold fallback, success save, repeat reuse and default-branch exclusion", (t) => {
  const f = fixture(t, { "Cargo.toml": "[package]" });
  const config = s3;
  const primary = f.plan(config, { ref: "refs/heads/trunk" }).caches.cargo;
  const pr = f.plan(config, { pr: 12, headRepository: context.repository }).caches.cargo;
  const other = f.plan(config, { pr: 13, headRepository: context.repository }).caches.cargo;
  const entries = new Map([[primary.key, "default"]]);
  const restore = (cache) =>
    [cache.key, ...cache.restore.split("\n")].flatMap((prefix) =>
      [...entries.keys()].filter((key) => key.startsWith(prefix)),
    )[0];
  assert.equal(restore(pr), primary.key);
  entries.set(pr.key, "successful PR");
  assert.equal(restore(pr), pr.key);
  assert.equal(restore(primary), primary.key);
  entries.delete(primary.key);
  assert.equal(restore(primary), undefined);
  assert.equal(restore(other), undefined);
  assert.notEqual(f.plan(config, { ref: "refs/heads/other" }).caches.cargo.key, f.plan(config).caches.cargo.key);
  assert.notEqual(f.plan(config, { repository: "other/project" }).caches.cargo.restore, pr.restore);
});

test("concurrent identical dependency writers intentionally share an immutable logical key", (t) => {
  const f = fixture(t, { "Cargo.toml": "[package]" });
  assert.equal(f.plan().caches.cargo.key, f.plan().caches.cargo.key);
  assert.notEqual(
    f.plan({ "cargo-target": "true", "cargo-build-variant": "one" }).caches["cargo-target"].key,
    f.plan({ "cargo-target": "true", "cargo-build-variant": "two" }).caches["cargo-target"].key,
  );
});
