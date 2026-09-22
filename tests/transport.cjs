// Optional integration check: reviewed upstream code against a disposable local S3 endpoint.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { spawn } = require('node:child_process');

const revision = '88d90644011a3a9957fd141a106f5a94f9794203';
async function main() {
  const scratch = path.resolve('.tars/scratch');
  fs.mkdirSync(scratch, { recursive: true });
  const root = fs.mkdtempSync(path.join(scratch, 's3-transport-'));
  let requests = 0;
  const server = http.createServer((request, response) => {
    requests++;
    request.resume();
    response.writeHead(403, { 'content-type': 'application/xml' });
    response.end('<Error><Code>AccessDenied</Code><Message>Disposable denial fixture</Message></Error>');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const cache = path.join(root, 'downloads');
    fs.mkdirSync(cache);
    fs.writeFileSync(path.join(cache, 'dependency'), 'fixture');
    for (const name of ['output', 'state']) fs.writeFileSync(path.join(root, name), '');
    const env = {
      ...process.env, GITHUB_REF: 'refs/heads/test', GITHUB_REPOSITORY: 'fixture/actions',
      GITHUB_WORKSPACE: root, RUNNER_TEMP: root, GITHUB_OUTPUT: path.join(root, 'output'), GITHUB_STATE: path.join(root, 'state'),
      INPUT_PATH: cache, INPUT_KEY: 'fixture-test-key', 'INPUT_RESTORE-KEYS': 'fixture-',
      INPUT_ENABLECROSSOSARCHIVE: 'false', 'INPUT_LOOKUP-ONLY': 'false', 'INPUT_FAIL-ON-CACHE-MISS': 'false',
      RUNS_ON_S3_BUCKET_CACHE: 'fixture', RUNS_ON_S3_BUCKET_ENDPOINT: `http://127.0.0.1:${server.address().port}`,
      RUNS_ON_S3_FORCE_PATH_STYLE: 'true', RUNS_ON_RUNNER_NAME: '', RUNS_ON_AWS_REGION: '',
      AWS_REGION: 'us-east-1', AWS_ACCESS_KEY_ID: 'fixture-access-key', AWS_SECRET_ACCESS_KEY: 'fixture-secret-key',
      AWS_SESSION_TOKEN: '', AWS_PROFILE: '', AWS_WEB_IDENTITY_TOKEN_FILE: '', AWS_EC2_METADATA_DISABLED: 'true',
      AWS_MAX_ATTEMPTS: '1',
    };
    for (const phase of ['restore', 'save']) {
      const response = await fetch(`https://raw.githubusercontent.com/runs-on/cache/${revision}/dist/${phase}/index.js`);
      assert(response.ok, `Cannot fetch reviewed ${phase} action: ${response.status}`);
      const script = path.join(root, `${phase}.cjs`);
      fs.writeFileSync(script, await response.text());
      const before = requests;
      const result = await new Promise((resolve, reject) => {
        const child = spawn(process.execPath, [script], { env, cwd: root, timeout: 30000 });
        let output = '';
        child.stdout.on('data', data => { output += data; });
        child.stderr.on('data', data => { output += data; });
        child.on('error', reject);
        child.on('close', code => resolve({ code, output }));
      });
      assert.equal(result.code, 0, result.output);
      assert(requests > before, `${phase} must contact the disposable S3 endpoint`);
      assert.match(result.output, /AccessDenied|Disposable denial fixture/);
      if (phase === 'save') assert.match(result.output, /::warning::/);
      assert(!result.output.includes('fixture-secret-key'), 'Secret must not appear in transport diagnostics');
      console.log(`Pinned S3 ${phase}: denied request remained nonfatal; ${requests - before} local request(s).`);
    }
  } finally {
    await new Promise(resolve => server.close(resolve));
    fs.rmSync(root, { recursive: true, force: true });
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
