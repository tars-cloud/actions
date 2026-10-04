import { appendFileSync, readFileSync } from "node:fs";

const input = (name) => process.env[`INPUT_${name.toUpperCase()}`] ?? "";
const output = (name, value) => appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
function resolve() {
  try {
    const profile = input("secretspec-profile");
    if (profile && !/^[a-zA-Z0-9_-]+$/.test(profile)) throw new Error("Invalid SecretSpec profile name");
    if (profile) appendFileSync(process.env.GITHUB_ENV, `SECRETSPEC_PROFILE=${profile}\n`);
    const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"));
    const repository = process.env.GITHUB_REPOSITORY;
    const pr = event.pull_request;
    const trusted =
      process.env.GITHUB_EVENT_NAME !== "pull_request_target" &&
      process.env.GITHUB_ACTOR !== "dependabot[bot]" &&
      pr?.user?.login !== "dependabot[bot]" &&
      (!pr || pr.head?.repo?.full_name?.toLowerCase() === repository.toLowerCase());
    output("trusted", trusted);
    const repositories = JSON.parse(input("dependency-repositories") || "[]");
    if (
      !Array.isArray(repositories) ||
      repositories.some((name) => typeof name !== "string" || !/^[a-zA-Z0-9_.-]+$/.test(name))
    )
      throw new Error("dependency-repositories must be a JSON array of repository names in one owner");
    if (!trusted) {
      output("app-enabled", false);
      output("checkout-with-app", false);
      output("repositories", "");
      return;
    }
    const id = input("app-id");
    const key = input("app-private-key");
    const enabled = Boolean(id || key);
    if (enabled && (!id || !key || !repositories.length))
      throw new Error("Dependency App authentication requires an App ID, private key and named repositories");
    if (enabled && input("dependency-token")) throw new Error("Choose a dependency token or App authentication");
    const owner = input("dependency-owner") || repository.split("/")[0];
    if (!/^[a-zA-Z0-9-]+$/.test(owner)) throw new Error("Invalid dependency owner");
    const sameOwner = owner.toLowerCase() === repository.split("/")[0].toLowerCase();
    output("app-enabled", enabled);
    output(
      "checkout-with-app",
      enabled &&
        sameOwner &&
        repositories.some((name) => name.toLowerCase() === repository.split("/")[1].toLowerCase()),
    );
    output("repositories", [...new Set(repositories)].join(","));
  } catch (error) {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  }
}
resolve();
