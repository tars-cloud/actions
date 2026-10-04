use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn context(
    root: &Path,
    event: &Value,
    actor: &str,
    inputs: &Value,
    event_name: &str,
) -> Result<(bool, String, String)> {
    let scratch = root.join(".tars/scratch/consumer-context");
    fs::create_dir_all(&scratch)?;
    let fixture = tempfile::tempdir_in(scratch)?;
    let directory = fixture.path();
    fs::write(directory.join("event.json"), serde_json::to_vec(event)?)?;
    fs::write(directory.join("output"), "")?;
    fs::write(directory.join("env"), "")?;
    let mut command = Command::new(crate::runner::executable("node")?);
    command
        .arg(root.join("composite/setup-consumer/scripts/context/main.mjs"))
        .env_clear()
        .env("GITHUB_EVENT_PATH", directory.join("event.json"))
        .env("GITHUB_EVENT_NAME", event_name)
        .env("GITHUB_REPOSITORY", "example/consumer")
        .env("GITHUB_ACTOR", actor)
        .env("GITHUB_OUTPUT", directory.join("output"))
        .env("GITHUB_ENV", directory.join("env"));
    for (key, value) in inputs.as_object().context("context inputs")? {
        command.env(
            format!("INPUT_{}", key.to_uppercase()),
            value.as_str().context("context string input")?,
        );
    }
    let output = command.output()?;
    let logs = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        !logs.contains("fixture-private-key") && !logs.contains("fixture-dependency-token"),
        "context must never log supplied credentials"
    );
    Ok((
        output.status.success(),
        fs::read_to_string(directory.join("output"))?,
        fs::read_to_string(directory.join("env"))?,
    ))
}

fn trust(root: &Path) -> Result<()> {
    let inputs = json!({"app-id":"fixture-app","app-private-key":"fixture-private-key","dependency-repositories":"[\"private-input\"]","secretspec-profile":"ci"});
    for (actor, author, head, trusted) in [
        ("developer", "developer", "example/consumer", true),
        ("developer", "developer", "EXAMPLE/Consumer", true),
        ("dependabot[bot]", "developer", "example/consumer", false),
        ("developer", "dependabot[bot]", "example/consumer", false),
        ("developer", "contributor", "fork/consumer", false),
    ] {
        let event =
            json!({"pull_request":{"user":{"login":author},"head":{"repo":{"full_name":head}}}});
        let (success, outputs, environment) =
            context(root, &event, actor, &inputs, "pull_request")?;
        ensure!(
            success && outputs.contains(&format!("trusted={trusted}\n")),
            "credential trust decision"
        );
        ensure!(
            outputs.contains(&format!("app-enabled={trusted}\n")),
            "untrusted runs must not mint tokens"
        );
        ensure!(
            environment == "SECRETSPEC_PROFILE=ci\n",
            "profile must be available before shell evaluation"
        );
        if trusted {
            ensure!(
                outputs.contains("repositories=private-input\n")
                    && outputs.contains("checkout-with-app=false\n"),
                "only named dependencies may enter App scope"
            );
        } else {
            ensure!(
                outputs.contains("repositories=\n"),
                "untrusted runs must not receive App scope"
            );
        }
    }
    for (event_name, trusted) in [("pull_request_target", false), ("push", true)] {
        let (success, outputs, _) = context(root, &json!({}), "developer", &inputs, event_name)?;
        ensure!(
            success && outputs.contains(&format!("trusted={trusted}\n")),
            "event-specific credential trust"
        );
        ensure!(
            outputs.contains(&format!("app-enabled={trusted}\n")),
            "event-specific token creation"
        );
    }
    Ok(())
}

fn policies(root: &Path) -> Result<()> {
    let inputs = json!({"app-id":"fixture-app","app-private-key":"fixture-private-key","dependency-repositories":"[\"private-input\"]"});
    for (field, value) in [
        ("dependency-repositories", "[]"),
        ("dependency-repositories", "{}"),
        ("dependency-repositories", "[123]"),
        ("dependency-repositories", "[\"owner/repo\"]"),
        ("dependency-repositories", "[\"repo\\ninjected=true\"]"),
        ("dependency-owner", "owner/repo"),
        ("app-private-key", ""),
        ("dependency-token", "fixture-dependency-token"),
        ("secretspec-profile", "ci\nINJECTED=true"),
    ] {
        let mut invalid = inputs.clone();
        invalid[field] = json!(value);
        ensure!(
            !context(root, &json!({}), "developer", &invalid, "push")?.0,
            "invalid context {field} must fail before checkout"
        );
    }
    let mut explicit = inputs.clone();
    explicit["dependency-repositories"] = json!("[\"private-input\",\"consumer\"]");
    let (success, outputs, _) = context(root, &json!({}), "developer", &explicit, "push")?;
    ensure!(
        success
            && outputs.contains("checkout-with-app=true\n")
            && outputs.contains("repositories=private-input,consumer\n"),
        "explicit consumer scope authenticates private checkout"
    );
    let mut foreign = inputs;
    foreign["dependency-owner"] = json!("other-owner");
    let (success, outputs, environment) = context(root, &json!({}), "developer", &foreign, "push")?;
    ensure!(
        success
            && outputs.contains("checkout-with-app=false\n")
            && outputs.contains("repositories=private-input\n"),
        "foreign dependency installation must not authenticate the consumer checkout"
    );
    ensure!(
        environment.is_empty(),
        "empty profile retains consumer configuration"
    );
    let (success, outputs, _) = context(root, &json!({}), "developer", &json!({}), "push")?;
    ensure!(
        success && outputs.contains("app-enabled=false\n"),
        "default context must not require App secrets"
    );
    Ok(())
}

pub(super) fn contracts(root: &Path) -> Result<()> {
    let checkout = super::workflows::load(
        root,
        "composite/setup-consumer/scripts/checkout/action.yaml",
    )?;
    let steps = checkout["runs"]["steps"]
        .as_array()
        .context("checkout steps")?;
    ensure!(
        steps[1]["if"] == "steps.resolve_context.outputs.app-enabled == 'true'"
            && steps[1]["with"]["permission-contents"] == "read",
        "dependency App credentials require trust and read-only Contents permission"
    );
    ensure!(
        steps[2]["with"]["persist-credentials"] == false,
        "checkout must not persist credentials"
    );
    for name in ["lfs", "ref", "fetch-depth", "submodules"] {
        ensure!(
            steps[2]["with"][name] == format!("${{{{ inputs.{name} }}}}"),
            "checkout must forward {name}"
        );
    }
    let setup = super::workflows::load(root, "composite/setup-consumer/action.yaml")?;
    let steps = setup["runs"]["steps"].as_array().context("setup steps")?;
    ensure!(
        steps[0]["uses"] == "$/composite/setup-consumer/scripts/checkout"
            && steps[1]["uses"] == "$/composite/setup-cache"
            && steps[2]["uses"] == "$/composite/setup-devenv",
        "setup must compose checkout, cache and environment at the selected revision"
    );
    for key in [
        "s3-endpoint",
        "s3-bucket",
        "s3-region",
        "s3-access-key",
        "s3-secret-key",
        "s3-session-token",
    ] {
        ensure!(
            steps[1]["with"][key]
                .as_str()
                .is_some_and(|value| value.contains("outputs.trusted == 'true'")),
            "untrusted runs must not receive cache credentials"
        );
    }
    Ok(())
}

pub(super) fn run(root: &Path) -> Result<()> {
    contracts(root)?;
    trust(root)?;
    policies(root)?;
    println!("PASS consumer setup: credential scope, trust, checkout and SecretSpec profiles");
    Ok(())
}
