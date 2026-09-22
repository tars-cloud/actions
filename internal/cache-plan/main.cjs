const fs = require('node:fs');
const path = require('node:path');
const { createHash, randomUUID } = require('node:crypto');

const excluded = new Set([
  '.git', '.devenv', '.direnv', '.tars', 'scratch', 'node_modules',
  'target', 'vendor', 'vendors', '.venv', 'venv', '__pycache__',
  'dist', 'build', '.cache', '.bun', '.cargo', '.next', 'coverage',
]);
const digest = value => createHash('sha256').update(value).digest('hex').slice(0, 24);
const truth = (value, name) => {
  if (!['true', 'false'].includes(value)) throw new Error(`${name} must be true or false.`);
  return value === 'true';
};

function policy(config, context) {
  if (context.os !== 'Linux' || !['X64', 'ARM64'].includes(context.arch)) {
    throw new Error('Supported platforms are Linux X64 and ARM64.');
  }
  if (!['github-hosted', 'self-hosted'].includes(context.runner)) throw new Error('Unknown runner.environment.');
  const fork = Boolean(context.headRepository && context.headRepository.toLowerCase() !== context.repository.toLowerCase());
  const fields = ['s3-endpoint', 's3-bucket', 's3-region', 's3-access-key', 's3-secret-key'];
  let backend = 'github';
  if (!fork && context.runner === 'self-hosted') {
    const intended = [...fields, 's3-session-token'].some(key => config[key]);
    if (intended) {
      const missing = fields.filter(key => !config[key]);
      if (missing.length) throw new Error(`Incomplete S3 configuration; supply: ${missing.join(', ')}.`);
      const endpoint = new URL(config['s3-endpoint']);
      if (!['https:', 'http:'].includes(endpoint.protocol) || endpoint.username || endpoint.password) {
        throw new Error('s3-endpoint must be an HTTP(S) endpoint without embedded credentials.');
      }
      backend = 's3';
      truth(config['s3-force-path-style'] || 'true', 's3-force-path-style');
    }
  }
  const name = config['cachix-name'] || '';
  if (name && !/^[a-z0-9][a-z0-9-]*$/.test(name)) throw new Error('Invalid cachix-name.');
  return { backend, fork, cachix: name ? (!fork && config['cachix-token'] ? 'write' : 'read') : 'disabled' };
}

function discover(root, patterns) {
  const files = [];
  function visit(directory, relative = '') {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const name = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isSymbolicLink() || excluded.has(entry.name)) continue;
      if (patterns.some(pattern => path.matchesGlob(name, pattern) || path.matchesGlob(entry.name, pattern))) continue;
      if (entry.isDirectory()) visit(path.join(directory, entry.name), name);
      else if (entry.isFile()) files.push(name);
    }
  }
  visit(root);
  return files;
}

function cachePlan(config, context, env = process.env, now = new Date()) {
  const selection = policy(config, context);
  const type = config.type || 'devenv';
  if (!['devenv', 'flakes'].includes(type)) throw new Error('type must be devenv or flakes.');
  const shell = config['flake-shell'] || '.#default';
  if (shell.startsWith('-')) throw new Error('flake-shell must be a flake selector, not a command option.');
  const root = fs.realpathSync(path.resolve(context.workspace, config['working-directory'] || '.'));
  const required = type === 'devenv' ? ['devenv.nix', 'devenv.yaml', 'devenv.lock'] : ['flake.nix', 'flake.lock'];
  for (const file of required) {
    const full = path.join(root, file);
    if (!fs.existsSync(full) || !fs.lstatSync(full).isFile()) throw new Error(`Selected environment requires ${file} in working-directory (regular files only).`);
  }
  const files = discover(root, (config.exclude || '').split(/\r?\n/).map(s => s.trim()).filter(Boolean));
  const base = name => path.basename(name);
  const read = name => fs.readFileSync(path.join(root, name));
  const fingerprint = names => digest([...new Set(names)].sort().map(name => `${name}\0${read(name).toString('base64')}`).join('\0'));
  const reasons = [];
  const automatic = !config.tools || config.tools === 'auto';
  const requested = automatic ? [] : config.tools.split(/[\s,]+/).filter(Boolean);
  if (requested.some(tool => !['cargo', 'python', 'bun', 'trivy', 'none'].includes(tool)) || (requested.includes('none') && requested.length !== 1)) {
    throw new Error('tools must be auto, none, or a list of cargo, python, bun, trivy.');
  }
  const has = regex => files.some(name => regex.test(base(name)));
  const cargo = has(/^Cargo\.toml$/);
  const uv = has(/^uv\.lock$/);
  const pip = has(/^requirements(?:[-.].*)?\.txt$/);
  const py = has(/^pyproject\.toml$/) || uv || pip;
  const bun = has(/^bun\.lockb?$/) || files.filter(name => base(name) === 'package.json').some(name => {
    try { return /^bun@/.test(JSON.parse(read(name)).packageManager || ''); }
    catch { reasons.push(`Cannot parse ${name}; use tools: bun if appropriate.`); return false; }
  });
  const trivyConfig = name => !/(^|\/)\.github\/workflows\//.test(name) && /^trivy\.ya?ml$/.test(base(name));
  const trivy = files.some(trivyConfig);
  const active = tool => automatic ? ({ cargo, python: py, bun, trivy })[tool] : requested.includes(tool);
  const manager = config['python-manager'] || 'auto';
  if (!['auto', 'uv', 'pip'].includes(manager)) throw new Error('python-manager must be auto, uv or pip.');
  const selected = [];
  for (const tool of ['cargo', 'bun', 'trivy']) if (active(tool)) selected.push(tool);
  if (active('python')) {
    if (manager !== 'auto') selected.push(manager);
    else {
      if (uv) selected.push('uv');
      if (pip) selected.push('pip');
      if (!uv && !pip) reasons.push('Python manager is ambiguous; set python-manager: uv or pip.');
    }
  }
  if (automatic && !bun && has(/^package\.json$/)) reasons.push('package.json alone does not identify Bun; use tools: bun.');
  const target = truth(config['cargo-target'] || 'false', 'cargo-target');
  if (target && selected.includes('cargo')) selected.push('cargo-target');
  const home = env.HOME;
  if (!home) throw new Error('HOME is required for Linux cache defaults.');
  function directory(input, variable, fallback) {
    let value = config[input] || env[variable] || fallback;
    if (value.startsWith('~/')) value = path.join(home, value.slice(2));
    if (/[\r\n\0*?!\[\]{}]/.test(value)) throw new Error(`${input} must be one literal directory, without glob patterns or line breaks.`);
    const resolved = path.resolve(root, value);
    if (resolved === '/' || resolved === home || resolved === root) throw new Error(`${input} must be a dedicated cache directory.`);
    return resolved;
  }
  const xdg = env.XDG_CACHE_HOME || path.join(home, '.cache');
  const cargoHome = selected.includes('cargo') ? directory('cargo-cache-path', 'CARGO_HOME', path.join(home, '.cargo')) : '';
  const locations = {
    uv: ['uv-cache-path', 'UV_CACHE_DIR', path.join(xdg, 'uv')],
    pip: ['pip-cache-path', 'PIP_CACHE_DIR', path.join(xdg, 'pip')],
    bun: ['bun-cache-path', 'BUN_INSTALL_CACHE_DIR', path.join(env.BUN_INSTALL || path.join(home, '.bun'), 'install/cache')],
    trivy: ['trivy-cache-path', 'TRIVY_CACHE_DIR', path.join(xdg, 'trivy')],
    'cargo-target': ['cargo-target-path', 'CARGO_TARGET_DIR', path.join(root, 'target')],
  };
  const paths = {};
  for (const tool of selected) paths[tool] = tool === 'cargo'
    ? ['registry/index', 'registry/cache', 'git/db'].map(part => path.join(cargoHome, part))
    : [directory(...locations[tool])];
  const lock = type === 'devenv' ? 'devenv.lock' : 'flake.lock';
  const compatibility = digest(`${type}\0${type === 'flakes' ? shell : ''}\0${fingerprint([lock])}`);
  if (!context.repository || !context.defaultBranch || !context.ref) throw new Error('Repository, default branch and ref context are required.');
  const scope = context.pr ? `refs/pull/${context.pr}/merge` : context.ref;
  const defaultScope = `refs/heads/${context.defaultBranch}`;
  const prefix = `tars-v1-${digest(context.repository.toLowerCase())}-${context.os}-${context.arch}-${compatibility}`;
  const scopePrefix = ref => `${prefix}-${digest(ref)}`;
  const patterns = {
    cargo: /^(Cargo\.(toml|lock))$/,
    'cargo-target': /^(Cargo\.(toml|lock)|rust-toolchain(\.toml)?|config(\.toml)?)$/,
    uv: /^(uv\.lock|pyproject\.toml|uv\.toml)$/,
    pip: /^(requirements(?:[-.].*)?\.(txt|in)|pyproject\.toml|setup\.(cfg|py))$/,
    bun: /^(bun\.lockb?|package\.json|bunfig\.toml)$/,
    trivy: /^trivy\.ya?ml$/,
  };
  const caches = {};
  for (const tool of selected) {
    const relevant = files.filter(name => tool === 'trivy' ? trivyConfig(name) : patterns[tool].test(base(name)));
    const build = tool === 'cargo-target' ? digest(JSON.stringify([
      fingerprint(files.filter(name => /^(rust-toolchain(\.toml)?|config(\.toml)?)$/.test(base(name)))),
      config['cargo-build-variant'] || '', config['cargo-build-target'] || env.CARGO_BUILD_TARGET || '',
      env.RUSTFLAGS || '', env.CARGO_ENCODED_RUSTFLAGS || '',
    ])) : 'downloads';
    const suffix = `${tool}-${build}-`;
    const current = `${scopePrefix(scope)}-${suffix}`;
    const fallback = `${scopePrefix(defaultScope)}-${suffix}`;
    const content = tool === 'trivy' ? `${fingerprint(relevant)}-${now.toISOString().slice(0, 10)}` : fingerprint(relevant);
    caches[tool] = {
      path: paths[tool].join('\n'), key: `${current}${content}`,
      restore: [...new Set([current, fallback])].join('\n'),
    };
    reasons.push(`${tool}: ${automatic ? 'detected project metadata' : 'explicit tool selection'}; ${selection.backend} storage.`);
  }
  const exports = {};
  for (const tool of selected) {
    const variable = tool === 'cargo' ? 'CARGO_HOME' : locations[tool][1];
    exports[variable] = tool === 'cargo' ? cargoHome : paths[tool][0];
  }
  return { ...selection, tools: selected, caches, reasons, exports };
}

function writeValue(file, name, value) {
  const delimiter = randomUUID();
  fs.appendFileSync(file, `${name}<<${delimiter}\n${value}\n${delimiter}\n`);
}

if (require.main === module) {
  try {
    const plan = cachePlan(JSON.parse(process.env.INPUT_CONFIG), JSON.parse(process.env.INPUT_CONTEXT));
    writeValue(process.env.GITHUB_OUTPUT, 'plan', JSON.stringify(plan));
    for (const [key, value] of Object.entries(plan.exports)) writeValue(process.env.GITHUB_ENV, key, value);
    for (const reason of plan.reasons) console.log(`::notice::${reason.replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A')}`);
    console.log(`Archive backend: ${plan.backend}; Cachix: ${plan.cachix}; fork: ${plan.fork}.`);
  } catch (error) {
    console.error(`::error::${error.message.replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A')}`);
    process.exitCode = 1;
  }
}
module.exports = { policy, discover, cachePlan };
