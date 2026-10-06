use super::workflows::{execute, inputs, load, step};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

pub(super) fn contracts(root: &Path) -> Result<()> {
    let workflow = load(root, ".github/workflows/consumer-devenv-update.yaml")?;
    ensure!(
        workflow["on"]["workflow_call"]["inputs"]["job-name"]["default"]
            == "Update Devenv Dependencies"
            && workflow["jobs"]["update"]["name"] == "${{ inputs.job-name }}",
        "preserve the default update name and allow callers to distinguish variants"
    );
    let steps = workflow["jobs"]["update"]["steps"]
        .as_array()
        .context("steps")?;
    let position = |id| steps.iter().position(|s| s["id"] == id).unwrap();
    for (before, after) in [
        ("configuration", "checkout"),
        ("paths", "devenv"),
        ("update", "snapshot"),
        ("snapshot", "validate"),
        ("validate", "scope"),
        ("scope", "app-token"),
        ("app-token", "pull-request"),
    ] {
        ensure!(
            position(before) < position(after),
            "{before} must precede {after}"
        );
    }
    ensure!(
        workflow["permissions"] == json!({"contents":"read"}),
        "read-only job token"
    );
    for (id, action) in [
        ("devenv", "setup-devenv"),
        ("update", "run-devenv"),
        ("validate", "run-devenv"),
    ] {
        let s = step(&workflow, "update", id)?;
        ensure!(
            s["uses"] == format!("$/composite/{action}"),
            "same-revision environment action"
        );
        for field in ["type", "working-directory", "flake-shell"] {
            ensure!(
                s["with"][field] == format!("${{{{ inputs.{field} }}}}"),
                "consistent environment: {id}/{field}"
            );
        }
        ensure!(
            s.get("continue-on-error").is_none(),
            "validation must gate publication"
        );
    }
    for id in ["app-token", "pull-request"] {
        ensure!(
            step(&workflow, "update", id)?["if"] == "${{ !inputs.dry-run }}",
            "publish only after success, including unchanged updates for reconciliation"
        );
    }
    let pr = step(&workflow, "update", "pull-request")?;
    ensure!(
        pr["with"]["token"] == "${{ steps.app-token.outputs.token }}",
        "no workflow-token fallback"
    );
    ensure!(
        pr["with"]["add-paths"] == "${{ steps.paths.outputs.lockfile }}",
        "commit only the selected lockfile"
    );
    ensure!(
        pr["with"]["branch"] == "${{ inputs.branch }}" && pr["with"].get("branch-suffix").is_none(),
        "reuse one branch"
    );
    ensure!(
        pr["with"]["delete-branch"] == true && pr["with"]["sign-commits"] == true,
        "signed commits and stale branch cleanup"
    );
    let ci = load(root, ".github/workflows/repository-ci.yaml")?;
    let caller = &ci["jobs"]["devenv-update"];
    ensure!(
        caller["uses"] == "./.github/workflows/consumer-devenv-update.yaml"
            && caller["with"]["dry-run"] == true,
        "CI calls exact workflow revision without publication"
    );
    ensure!(
        caller["strategy"]["matrix"]["type"] == json!(["devenv", "flakes"]),
        "exercise both update modes"
    );
    ensure!(
        caller["with"]["job-name"]
            == "Update Devenv Dependencies - ${{ matrix.type == 'devenv' && 'Devenv' || 'Flake' }} - ${{ matrix.architecture }}",
        "label every update variant by environment and architecture"
    );
    let lifecycle = load(root, ".github/workflows/test-devenv-update-lifecycle.yaml")?;
    let probe = load(root, ".github/workflows/test-devenv-update-probe.yaml")?;
    ensure!(
        probe["on"]["pull_request"]["branches"] == json!(["tact-update-base-[0-9]+-[0-9]+"]),
        "Devenv PR probe must match only numeric update fixture identities, excluding CRAP lifecycle branches"
    );
    for id in ["create", "repeat", "refresh", "close"] {
        let call = &lifecycle["jobs"][id];
        ensure!(
            call["uses"] == "./.github/workflows/consumer-devenv-update.yaml",
            "lifecycle must test the current revision"
        );
        ensure!(
            call["with"].get("dry-run").is_none(),
            "lifecycle must publish real PRs"
        );
        inputs(call, &workflow["on"]["workflow_call"])?;
    }
    ensure!(
        lifecycle["jobs"]["cleanup"]["if"] == "always() && needs.prepare.result != 'skipped'",
        "failed lifecycle runs must clean up"
    );
    Ok(())
}

pub(super) fn run(root: &Path) -> Result<()> {
    let workflow = load(root, ".github/workflows/consumer-devenv-update.yaml")?;
    let base = json!({
        "tools":["git"],
        "env":{"GITHUB_EVENT_NAME":"schedule","DRY_RUN":"false","APP_ID":"example-app","APP_PRIVATE_KEY":"fixture-key","ENVIRONMENT_TYPE":"devenv","PROJECT_DIRECTORY":".","FLAKE_SHELL":".#default","VALIDATION_COMMAND":"","BASE_BRANCH":"trunk","UPDATE_BRANCH":"update-devenv-lock"},
        "expect":{"exit":0,"calls":[]}
    });
    execute(root, &workflow, "update", "configuration", base.clone())?;
    for (key, value) in [
        ("APP_ID", ""),
        ("APP_PRIVATE_KEY", ""),
        ("GITHUB_EVENT_NAME", "pull_request"),
        ("GITHUB_EVENT_NAME", "pull_request_target"),
        ("ENVIRONMENT_TYPE", "unknown"),
        ("ENVIRONMENT_TYPE", "flakes"),
        ("UPDATE_BRANCH", "trunk"),
        ("UPDATE_BRANCH", "bad\nbranch"),
        ("PROJECT_DIRECTORY", "/absolute"),
        ("PROJECT_DIRECTORY", "a,b"),
        ("PROJECT_DIRECTORY", "$(touch injected)"),
        ("PROJECT_DIRECTORY", "a\nkey=value"),
    ] {
        let mut case = base.clone();
        case["env"][key] = json!(value);
        // git check-ref-format reports invalid refs with exit 1.
        case["expect"]["exit"] = json!(1);
        execute(root, &workflow, "update", "configuration", case)?;
    }
    let mut case = base.clone();
    case["env"]["DRY_RUN"] = json!("true");
    case["env"]["GITHUB_EVENT_NAME"] = json!("pull_request");
    case["env"]["APP_ID"] = json!("");
    case["env"]["APP_PRIVATE_KEY"] = json!("");
    case["env"]["ENVIRONMENT_TYPE"] = json!("flakes");
    case["env"]["VALIDATION_COMMAND"] = json!("devenv-test");
    execute(root, &workflow, "update", "configuration", case.clone())?;
    case["env"]["FLAKE_SHELL"] = json!("github:example/other#default");
    case["expect"]["exit"] = json!(1);
    execute(root, &workflow, "update", "configuration", case)?;

    for (mode, command, args) in [
        ("devenv", "devenv", json!(["--no-tui", "update"])),
        ("flakes", "nix", json!(["flake", "update"])),
    ] {
        for exit in [0, 7] {
            execute(
                root,
                &workflow,
                "update",
                "update",
                json!({
                    "env":{"ENVIRONMENT_TYPE":mode},
                    "commands":{command:[{"args":args,"exit":exit}]},
                    "expect":{"exit":exit,"calls":[{"command":command,"args":args}]}
                }),
            )?;
        }
    }
    for exit in [0, 9] {
        execute(
            root,
            &workflow,
            "update",
            "validate",
            json!({
                "env":{"VALIDATION_COMMAND":""},
                "commands":{"devenv":[{"args":["--no-tui","test"],"exit":exit}]},
                "expect":{"exit":exit,"calls":[{"command":"devenv","args":["--no-tui","test"]}]}
            }),
        )?;
    }
    for (command, exit) in [
        ("printf validated > result", 0),
        ("exit 7", 7),
        ("false | true", 1),
    ] {
        execute(
            root,
            &workflow,
            "update",
            "validate",
            json!({
                "env":{"VALIDATION_COMMAND":command},"expect":{"exit":exit,"calls":[]}
            }),
        )?;
    }
    repositories(root, &workflow)
}

fn git(directory: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(crate::runner::executable("git")?)
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()?;
    ensure!(output.status.success(), "git {args:?}: {output:?}");
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn repositories(root: &Path, workflow: &Value) -> Result<()> {
    let scratch = root.join(".tars/scratch/tact");
    fs::create_dir_all(&scratch)?;
    for mode in ["devenv", "flakes"] {
        for scenario in [
            "unchanged",
            "updated",
            "staged",
            "generated",
            "tracked",
            "staged-other",
            "staged-reverted",
            "validation-lock",
            "commit",
            "deleted",
            "symlink",
            "escape",
            "untracked-lock",
        ] {
            let fixture = tempfile::Builder::new()
                .prefix("update-")
                .tempdir_in(&scratch)?;
            let repo = fixture.path().join("repo");
            let (relative, head) = prepare_repository(&repo, mode)?;
            let output_file = fixture.path().join("output");
            fs::write(&output_file, "")?;
            let run = |id: &str, hash: &str, directory: &str| -> Result<std::process::Output> {
                let s = step(workflow, "update", id)?["run"]
                    .as_str()
                    .context("script")?;
                Ok(Command::new(crate::runner::executable("bash")?)
                    .args(["--noprofile", "--norc", "-euo", "pipefail", "-c", s])
                    .current_dir(&repo)
                    .env("GITHUB_WORKSPACE", &repo)
                    .env("GITHUB_OUTPUT", &output_file)
                    .env("ENVIRONMENT_TYPE", mode)
                    .env("PROJECT_DIRECTORY", directory)
                    .env("LOCKFILE", &relative)
                    .env("ORIGINAL_HEAD", &head)
                    .env("UPDATED_HASH", hash)
                    .output()?)
            };
            if scenario == "escape" {
                symlink(fixture.path(), repo.join("outside"))?;
                ensure!(
                    !run("paths", "", "outside")?.status.success(),
                    "directory escape accepted"
                );
                continue;
            }
            if scenario == "untracked-lock" {
                git(&repo, &["rm", "--cached", "--", &relative])?;
                ensure!(
                    !run("paths", "", "nested project")?.status.success(),
                    "untracked lock accepted"
                );
                continue;
            }
            let paths = run("paths", "", "nested project")?;
            ensure!(paths.status.success(), "path resolution: {paths:?}");
            ensure!(
                fs::read_to_string(&output_file)?
                    .contains(&format!("lockfile={relative}\nhead={head}\n")),
                "wrong path outputs"
            );
            if scenario != "unchanged" {
                fs::write(repo.join(&relative), "updated\n")?;
            }
            let hash = git(&repo, &["hash-object", "--", &relative])?;
            mutate_repository(&repo, &relative, scenario)?;
            let output = run("scope", &hash, "nested project")?;
            let success = ["unchanged", "updated", "staged", "generated"].contains(&scenario);
            ensure!(
                output.status.success() == success,
                "{mode}/{scenario}: {output:?}"
            );
            if success {
                let expected = if scenario == "unchanged" {
                    "false"
                } else {
                    "true"
                };
                ensure!(
                    fs::read_to_string(&output_file)?.contains(&format!("changed={expected}\n")),
                    "change detection: {scenario}"
                );
            }
        }
    }
    Ok(())
}

fn prepare_repository(repo: &Path, mode: &str) -> Result<(String, String)> {
    fs::create_dir_all(repo.join("nested project"))?;
    git(repo, &["init", "--initial-branch=trunk"])?;
    git(repo, &["config", "user.name", "Fixture"])?;
    git(repo, &["config", "user.email", "fixture@example.invalid"])?;
    let lock = if mode == "devenv" {
        "devenv.lock"
    } else {
        "flake.lock"
    };
    let relative = format!("nested project/{lock}");
    fs::write(repo.join(&relative), "original\n")?;
    fs::write(repo.join("manifest"), "original\n")?;
    git(repo, &["add", "."])?;
    git(repo, &["commit", "-m", "fixture"])?;
    let head = git(repo, &["rev-parse", "HEAD"])?;
    Ok((relative, head))
}

fn mutate_repository(repo: &Path, relative: &str, scenario: &str) -> Result<()> {
    match scenario {
        "staged" => {
            git(repo, &["add", "--", relative])?;
        }
        "generated" => fs::write(repo.join("generated"), "excluded\n")?,
        "tracked" | "staged-other" | "staged-reverted" => {
            fs::write(repo.join("manifest"), "unexpected\n")?;
            if scenario != "tracked" {
                git(repo, &["add", "manifest"])?;
            }
            if scenario == "staged-reverted" {
                fs::write(repo.join("manifest"), "original\n")?;
            }
        }
        "validation-lock" => fs::write(repo.join(relative), "changed during validation\n")?,
        "commit" => {
            git(repo, &["add", "."])?;
            git(repo, &["commit", "-m", "unexpected"])?;
        }
        "deleted" => fs::remove_file(repo.join(relative))?,
        "symlink" => {
            fs::remove_file(repo.join(relative))?;
            symlink("../manifest", repo.join(relative))?;
        }
        _ => {}
    }
    Ok(())
}
