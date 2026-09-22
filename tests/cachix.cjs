// Exercise the pinned Cachix action's main/post lifecycle with fake bootstrap CLIs.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const { policy } = require('../internal/cache-plan/main.cjs');

function fileCommands(file) {
  const lines = fs.readFileSync(file, 'utf8').split('\n');
  const values = {};
  for (let i = 0; i < lines.length; i++) {
    const [name, delimiter] = lines[i].split('<<');
    if (!delimiter) continue;
    const value = [];
    while (++i < lines.length && lines[i] !== delimiter) value.push(lines[i]);
    values[name] = value.join('\n');
  }
  return values;
}

function run(command, args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, timeout: 30000 });
    let output = '';
    child.stdout.on('data', data => { output += data; });
    child.stderr.on('data', data => { output += data; });
    child.on('error', reject);
    child.on('close', code => resolve({ code, output }));
  });
}

async function main() {
  const scratch = path.resolve('.tars/scratch');
  fs.mkdirSync(scratch, { recursive: true });
  const root = fs.mkdtempSync(path.join(scratch, 'cachix-lifecycle-'));
  try {
    const response = await fetch('https://raw.githubusercontent.com/cachix/cachix-action/38b082610b782e7e93e209c35fd730d399dee866/dist/index.js');
    assert(response.ok, `Cannot fetch reviewed Cachix action: ${response.status}`);
    const script = path.join(root, 'action.cjs');
    fs.writeFileSync(script, await response.text());
    const bin = path.join(root, 'bin');
    fs.mkdirSync(bin);
    fs.writeFileSync(path.join(bin, 'cachix'), `#!/usr/bin/env bash
set -euo pipefail
case $1 in
  --version) echo 'cachix 1.12.1' ;;
  authtoken) echo authtoken >>"$TRACE" ;;
  *) printf '%s\\n' "$*" >>"$TRACE" ;;
esac
`, { mode: 0o755 });
    fs.writeFileSync(path.join(bin, 'nix'), `#!/usr/bin/env bash
set -euo pipefail
[[ $1 == show-config ]]
printf 'trusted-users = %s\\n' "$FIXTURE_USER"
`, { mode: 0o755 });
    for (const mode of ['read', 'write', 'fork']) {
      const directory = path.join(root, mode);
      fs.mkdirSync(directory);
      for (const name of ['state', 'env', 'trace']) fs.writeFileSync(path.join(directory, name), '');
      const token = mode === 'read' ? '' : 'fixture-token';
      const selected = policy({ 'cachix-name': 'public-fixture', 'cachix-token': token }, {
        os: 'Linux', arch: 'X64', runner: 'self-hosted', repository: 'fixture/project', headRepository: mode === 'fork' ? 'fork/project' : '',
      });
      const env = {
        ...process.env, HOME: directory, PATH: `${bin}:${process.env.PATH}`, RUNNER_TEMP: directory,
        GITHUB_STATE: path.join(directory, 'state'), GITHUB_ENV: path.join(directory, 'env'), TRACE: path.join(directory, 'trace'), FIXTURE_USER: os.userInfo().username,
        INPUT_NAME: 'public-fixture', INPUT_AUTHTOKEN: selected.cachix === 'write' ? token : '',
        INPUT_SKIPPUSH: String(selected.cachix !== 'write'), INPUT_USEDAEMON: 'true',
        INPUT_SKIPADDINGSUBSTITUTER: 'false', INPUT_CACHIXBIN: path.join(bin, 'cachix'),
        INPUT_SIGNINGKEY: '', INPUT_EXTRAPULLNAMES: '', INPUT_PATHSTOPUSH: '',
        INPUT_PUSHFILTER: '', INPUT_CACHIXARGS: '', CACHIX_AUTH_TOKEN: '', CACHIX_SIGNING_KEY: '',
        NIX_CONF: '', NIX_USER_CONF_FILES: '',
      };
      const mainResult = await run(process.execPath, [script], env);
      assert.equal(mainResult.code, 0, mainResult.output);
      const state = fileCommands(env.GITHUB_STATE);
      const exported = fileCommands(env.GITHUB_ENV);
      const postEnv = { ...env, ...exported, ...Object.fromEntries(Object.entries(state).map(([key, value]) => [`STATE_${key}`, value])) };
      if (selected.cachix === 'write') {
        assert.equal(state.pushMode, 'Daemon');
        const hook = path.join(exported.CACHIX_DAEMON_DIR, 'post-build-hook.sh');
        const pushed = await run('bash', [hook], { ...postEnv, OUT_PATHS: '/nix/store/fixture-output' });
        assert.equal(pushed.code, 0, pushed.output);
      } else assert.equal(state.pushMode, 'None');
      const postResult = await run(process.execPath, [script], postEnv);
      assert.equal(postResult.code, 0, postResult.output);
      const trace = fs.readFileSync(env.TRACE, 'utf8');
      assert.match(trace, /use public-fixture/);
      if (selected.cachix === 'write') {
        assert.match(trace, /authtoken/);
        assert.match(trace, /daemon push/);
        assert.match(trace, /daemon stop/);
      } else {
        assert(!/authtoken|daemon|push/.test(trace));
        assert(!mainResult.output.includes(token) || !token);
      }
      console.log(`Pinned Cachix ${mode}: substituter setup and post-job ${selected.cachix === 'write' ? 'daemon flush' : 'no-push'} verified with mocks.`);
    }
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
