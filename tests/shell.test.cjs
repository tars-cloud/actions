const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const internal = path.resolve("internal");
const scratch = path.resolve(".tars/scratch/tests");
fs.mkdirSync(scratch, { recursive: true });
function fixture(t) {
  const root = fs.mkdtempSync(path.join(scratch, "shell-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const bin = path.join(root, "bin");
  fs.mkdirSync(bin);
  for (const executable of ["bash", "dirname"]) {
    const found = spawnSync("bash", ["-c", 'command -v "$1"', "lookup", executable], {
      encoding: "utf8",
    }).stdout.trim();
    fs.symlinkSync(found, path.join(bin, executable));
  }
  const write = (file, value) => fs.writeFileSync(path.join(root, file), value, { mode: 0o755 });
  const mock = (name, body) => write(`bin/${name}`, "#!/usr/bin/env bash\nset -euo pipefail\n" + body + "\n");
  write("devenv.nix", "{}");
  write("devenv.yaml", "{}");
  write("devenv.lock", "{}");
  write("flake.nix", "{}");
  write("flake.lock", "{}");
  const env = {
    ...process.env,
    PATH: bin,
    HOME: root,
    RUNNER_OS: "Linux",
    RUNNER_ARCH: "X64",
    RUNNER_ENVIRONMENT: "self-hosted",
    GITHUB_WORKSPACE: root,
    PROJECT_DIRECTORY: root,
    GITHUB_OUTPUT: path.join(root, "output"),
    GITHUB_PATH: path.join(root, "paths"),
    TRACE: path.join(root, "trace"),
  };
  for (const key of ["buildInputs", "nativeBuildInputs", "propagatedBuildInputs", "propagatedNativeBuildInputs"])
    delete env[key];
  return {
    root,
    bin,
    env,
    write,
    mock,
    run: (name, more = {}) =>
      spawnSync("bash", [path.join(internal, name + ".sh")], { env: { ...env, ...more }, encoding: "utf8" }),
  };
}

test("Nix is reused on both architectures and consecutive calls never install", (t) => {
  const f = fixture(t);
  f.mock("nix", 'printf "%s\\n" "$*" >>"$TRACE"; [[ $1 == --version ]]');
  for (const arch of ["X64", "ARM64"])
    for (let i = 0; i < 2; i++) assert.equal(f.run("nix", { RUNNER_ARCH: arch }).status, 0);
  assert.equal(fs.readFileSync(path.join(f.root, "trace"), "utf8"), "--version\n".repeat(4));
  assert.equal(fs.readFileSync(path.join(f.root, "output"), "utf8"), "install=false\n".repeat(4));
});

test("missing Nix installs only on hosted runners; broken existing Nix fails", (t) => {
  const f = fixture(t);
  assert.match(f.run("nix").stdout, /Install Nix on this self-hosted runner/);
  assert.equal(f.run("nix").status, 1);
  assert.equal(f.run("nix", { RUNNER_ENVIRONMENT: "github-hosted" }).status, 0);
  assert.match(fs.readFileSync(path.join(f.root, "output"), "utf8"), /install=true/);
  f.mock("nix", "exit 42");
  assert.equal(f.run("nix").status, 42);
});

test("Cachix bootstrap reuses existing CLI or exposes a newly installed profile binary", (t) => {
  const f = fixture(t);
  f.mock("cachix", 'printf "%s\\n" "$*" >>"$TRACE"');
  assert.equal(f.run("cachix").status, 0);
  assert.equal(fs.readFileSync(path.join(f.root, "trace"), "utf8"), "--version\n");
  fs.unlinkSync(path.join(f.bin, "cachix"));
  fs.mkdirSync(path.join(f.root, ".nix-profile/bin"), { recursive: true });
  f.write(".nix-profile/bin/cachix", '#!/usr/bin/env bash\necho "cachix 1.12.1"\n');
  f.mock("nix", 'printf "%s\\n" "$*" >>"$TRACE"');
  assert.equal(f.run("cachix").status, 0);
  assert.match(fs.readFileSync(path.join(f.root, "trace"), "utf8"), /profile add nixpkgs#cachix/);
  assert.match(fs.readFileSync(path.join(f.root, "output"), "utf8"), /\.nix-profile\/bin\/cachix/);
});

test("unsupported platforms fail before cleanup, bootstrap or project commands", (t) => {
  const f = fixture(t);
  for (const script of ["cleanup", "nix", "devenv", "trivy"]) {
    assert.equal(f.run(script, { RUNNER_OS: "macOS" }).status, 1);
    assert.equal(f.run(script, { RUNNER_ARCH: "ARM" }).status, 1);
  }
  assert(!fs.existsSync(path.join(f.root, "trace")));
});

test("cleanup always skips self-hosted and limits hosted removal to named SDKs", (t) => {
  const f = fixture(t);
  f.mock("sudo", 'printf "%s\\n" "$*" >>"$TRACE"');
  f.mock("df", ":");
  assert.equal(f.run("cleanup", { ANDROID: "true", DOTNET: "true" }).status, 0);
  assert(!fs.existsSync(path.join(f.root, "trace")));
  assert.equal(f.run("cleanup", { RUNNER_ENVIRONMENT: "github-hosted", ANDROID: "true", DOTNET: "true" }).status, 0);
  const trace = fs.readFileSync(path.join(f.root, "trace"), "utf8");
  assert.equal(trace, "rm -rf -- /usr/local/lib/android\nrm -rf -- /usr/share/dotnet\n");
  assert(!/docker|toolcache|nix|node|llvm/.test(trace));
});

test("direct warmup reuses devenv and executes only a trivial command with safe arguments", (t) => {
  const f = fixture(t);
  f.mock("devenv", 'printf "%s\\n" "$@" >>"$TRACE"');
  assert.equal(f.run("devenv").status, 0);
  assert.equal(
    fs.readFileSync(path.join(f.root, "trace"), "utf8"),
    "--no-tui\n--version\n--no-tui\nshell\n--quiet\n--\nbash\n--noprofile\n--norc\n-euo\npipefail\n-c\n:\n",
  );
  fs.unlinkSync(path.join(f.root, "trace"));
  assert.equal(f.run("devenv", { WARMUP: "false" }).status, 0);
  assert.equal(fs.readFileSync(path.join(f.root, "trace"), "utf8"), "--no-tui\n--version\n");
});

test("warmup resolves relative working-directory from workspace on every dispatch", (t) => {
  const f = fixture(t);
  fs.mkdirSync(path.join(f.root, "nested"));
  for (const name of ["flake.nix", "flake.lock"])
    fs.renameSync(path.join(f.root, name), path.join(f.root, "nested", name));
  f.mock("nix", 'printf "%s" "$PWD" >"$TRACE"');
  const result = spawnSync("bash", [path.join(internal, "devenv.sh")], {
    cwd: f.root,
    encoding: "utf8",
    env: { ...f.env, PROJECT_DIRECTORY: "nested", ENVIRONMENT_TYPE: "flakes" },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(fs.readFileSync(path.join(f.root, "trace"), "utf8"), path.join(f.root, "nested"));
});

test("missing direct CLI installs once via profile add; flake warmup never needs devenv", (t) => {
  const f = fixture(t);
  fs.mkdirSync(path.join(f.root, ".nix-profile/bin"), { recursive: true });
  f.write(".nix-profile/bin/devenv", '#!/usr/bin/env bash\nprintf "%s\\n" "$*" >>"$TRACE"\n');
  f.mock("nix", 'printf "%s\\n" "$@" >>"$TRACE"');
  assert.equal(f.run("devenv", { WARMUP: "false" }).status, 0);
  assert.match(fs.readFileSync(path.join(f.root, "trace"), "utf8"), /^profile\nadd\nnixpkgs#devenv\n/);
  fs.unlinkSync(path.join(f.root, "trace"));
  fs.unlinkSync(path.join(f.root, "devenv.yaml"));
  for (const selector of [".#default", ".#named", ".#literal;$(touch injected)"]) {
    assert.equal(f.run("devenv", { ENVIRONMENT_TYPE: "flakes", FLAKE_SHELL: selector }).status, 0);
  }
  const trace = fs.readFileSync(path.join(f.root, "trace"), "utf8");
  assert.match(trace, /develop\n--impure\n.#named\n--command\nbash/);
  assert.match(trace, /literal;\$\(touch injected\)/);
  assert(!trace.split("\n").includes("profile"));
  assert(!fs.existsSync(path.join(f.root, "injected")));
});

test("shell dispatch preserves failures and noninteractive settings without losing profile", (t) => {
  const f = fixture(t);
  f.mock("nix", 'printf "%s:%s:%s" "$SECRETSPEC_PROVIDER" "$SECRETSPEC_ENV" "$SECRETSPEC_REASON" >"$TRACE"; exit 23');
  const result = f.run("devenv", {
    ENVIRONMENT_TYPE: "flakes",
    SECRETSPEC_PROVIDER: "interactive",
    SECRETSPEC_ENV: "custom",
    SECRETSPEC_REASON: "test-reason",
  });
  assert.equal(result.status, 23);
  assert.equal(fs.readFileSync(path.join(f.root, "trace"), "utf8"), "env:custom:test-reason");
});

test("Trivy rejects ambient binary and reports declared package version", (t) => {
  const f = fixture(t);
  f.mock("trivy", 'echo "ambient trivy"; exit 99');
  for (const dispatcher of ["devenv", "nix"])
    f.mock(
      dispatcher,
      'if [[ -n ${PROJECT_BIN:-} ]]; then export PATH="$PROJECT_BIN:$PATH"; fi; while [[ $1 != bash ]]; do shift; done; exec "$@"',
    );
  for (const type of ["devenv", "flakes"]) {
    const missing = f.run("trivy", { ENVIRONMENT_TYPE: type });
    assert.equal(missing.status, 1);
    assert.match(missing.stdout, /Add pkgs.trivy/);
    assert(!missing.stdout.includes("ambient trivy"));
  }
  fs.mkdirSync(path.join(f.root, "package/bin"), { recursive: true });
  f.write("package/bin/trivy", '#!/usr/bin/env bash\necho "Version: 0.99.0"\n');
  for (const type of ["devenv", "flakes"])
    assert.equal(f.run("trivy", { ENVIRONMENT_TYPE: type, PROJECT_BIN: path.join(f.root, "package/bin") }).status, 0);
  assert.equal(fs.readFileSync(path.join(f.root, "output"), "utf8"), "version=0.99.0\n".repeat(2));
});
