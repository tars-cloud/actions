const fs = require("node:fs");
const path = require("node:path");
const { randomUUID } = require("node:crypto");

const limit = 64 * 1024;

function output(name, value) {
  let delimiter;
  do {
    delimiter = `tars_${randomUUID()}`;
  } while (value.split(/\r?\n/).includes(delimiter));
  fs.appendFileSync(process.env.GITHUB_OUTPUT, `${name}<<${delimiter}\n${value}\n${delimiter}\n`);
}

function prepare(root) {
  const directory = fs.mkdtempSync(path.join(root, "tars-devenv-result-"));
  try {
    output("path", path.join(directory, "result.json"));
  } catch (error) {
    fs.rmdirSync(directory);
    throw error;
  }
}

function readResult(file) {
  let fd;
  try {
    fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  } catch (error) {
    if (error.code === "ENOENT") return "{}";
    throw new Error("DEVENV_RESULT_FILE must be a readable regular file, not a symlink.");
  }
  let bytes;
  try {
    if (!fs.fstatSync(fd).isFile()) throw new Error("DEVENV_RESULT_FILE must be a regular file.");
    const buffer = Buffer.alloc(limit + 1);
    const length = fs.readSync(fd, buffer, 0, buffer.length, 0);
    if (length > limit) throw new Error("DEVENV_RESULT_FILE exceeds the 64 KiB file limit.");
    bytes = buffer.subarray(0, length);
  } finally {
    fs.closeSync(fd);
  }
  let value;
  let result;
  try {
    result = new TextDecoder("utf-8", { fatal: true }).decode(bytes).trim();
    value = JSON.parse(result);
  } catch {
    throw new Error("DEVENV_RESULT_FILE must contain valid UTF-8 JSON; an empty file is invalid.");
  }
  if (value === null || Array.isArray(value) || typeof value !== "object") {
    throw new Error("DEVENV_RESULT_FILE must contain one JSON object.");
  }
  // Preserve numeric precision and formatting instead of reserializing through JavaScript numbers.
  if (Buffer.byteLength(result, "utf16le") > limit) {
    throw new Error("DEVENV_RESULT_FILE exceeds the 64 KiB UTF-16 output limit.");
  }
  return result;
}

function collect(root) {
  const file = path.resolve(process.env.INPUT_PATH || ".");
  const directory = path.dirname(file);
  if (
    path.dirname(directory) !== root ||
    !/^tars-devenv-result-[a-zA-Z0-9]{6}$/.test(path.basename(directory)) ||
    path.basename(file) !== "result.json"
  ) {
    throw new Error("Result path must belong to this action's temporary directory.");
  }
  const entry = fs.lstatSync(directory, { throwIfNoEntry: false });
  if (entry && (!entry.isDirectory() || entry.isSymbolicLink())) {
    throw new Error("Result directory must not be replaced with a file or symlink.");
  }
  const outcome = process.env.INPUT_OUTCOME;
  if (!["success", "failure", "cancelled", "skipped"].includes(outcome)) {
    throw new Error("Unknown consumer command outcome.");
  }
  let result;
  try {
    if (outcome === "success") result = readResult(file);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
  if (result !== undefined) output("result", result);
}

try {
  const root = fs.realpathSync(process.env.RUNNER_TEMP);
  if (process.env.INPUT_PHASE === "prepare") prepare(root);
  else if (process.env.INPUT_PHASE === "collect") collect(root);
  else throw new Error("Unknown result phase.");
} catch (error) {
  // Do not print parser excerpts, result contents or filesystem paths.
  const message = error.code ? "Could not manage the temporary devenv result file." : error.message;
  process.stdout.write(`::error::${message}\n`);
  process.exitCode = 1;
}
