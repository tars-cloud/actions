use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

pub(crate) fn mock() -> Option<Result<u8>> {
    let trace = std::env::var_os("TACT_CACHIX_TRACE")?;
    let executable = std::env::args_os().next()?;
    let name = Path::new(&executable).file_name()?.to_str()?;
    if !["nix", "cachix"].contains(&name) {
        return None;
    }
    Some((|| {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if name == "nix" {
            ensure!(args == ["show-config"], "unexpected Nix command: {args:?}");
            println!("trusted-users = {}", std::env::var("TACT_FIXTURE_USER")?);
        } else if args == ["--version"] {
            println!("cachix 1.12.1");
        } else {
            let line = if args.first().is_some_and(|s| s == "authtoken") {
                "authtoken".to_string()
            } else {
                args.join(" ")
            };
            writeln!(OpenOptions::new().append(true).open(trace)?, "{line}")?;
        }
        Ok(0)
    })())
}

pub(super) fn run(repository: &Path, root: &Path) -> Result<()> {
    let script = super::download(
        root,
        "https://raw.githubusercontent.com/cachix/cachix-action/38b082610b782e7e93e209c35fd730d399dee866/dist/index.js",
        "cachix.cjs",
    )?;
    let bin = root.join("bin");
    fs::create_dir(&bin)?;
    for name in ["cachix", "nix"] {
        symlink(std::env::current_exe()?, bin.join(name))?;
    }
    let user = Command::new("id").arg("-un").output()?;
    ensure!(user.status.success(), "cannot determine fixture user");
    let username = String::from_utf8(user.stdout)?.trim().to_string();
    let inherited = std::env::var("PATH")?;
    for mode in ["read", "write", "fork", "authenticated-read", "filtered"] {
        let directory = root.join(mode);
        fs::create_dir(&directory)?;
        for name in ["state", "env", "trace", "output"] {
            fs::write(directory.join(name), "")?;
        }
        let policy = Command::new("bash")
            .arg(repository.join("composite/setup-nix-cache/scripts/select.sh"))
            .env_clear()
            .env("PATH", &inherited)
            .env("GITHUB_OUTPUT", directory.join("output"))
            .env("CACHIX_NAME", "public-fixture")
            .env(
                "CACHIX_TOKEN",
                if mode == "read" { "" } else { "fixture-token" },
            )
            .env("REPOSITORY", "fixture/project")
            .env(
                "CACHIX_SKIP_PUSH",
                if mode == "authenticated-read" {
                    "true"
                } else {
                    "false"
                },
            )
            .env(
                "CACHIX_PUSH_FILTER",
                if mode == "filtered" {
                    r"(-source$|\.tar\.gz$|\.zip$)"
                } else {
                    ""
                },
            )
            .env(
                "HEAD_REPOSITORY",
                if mode == "fork" { "fork/project" } else { "" },
            )
            .output()?;
        ensure!(policy.status.success(), "Cachix policy failed");
        let selection = crate::output::values(&fs::read_to_string(directory.join("output"))?)?;
        let write = selection.get("cachix-mode").map(String::as_str) == Some("write");
        let authenticated =
            selection.get("cachix-authenticated").map(String::as_str) == Some("true");
        ensure!(
            write == (mode == "write" || mode == "filtered"),
            "unexpected Cachix selection: {selection:?}"
        );
        let mut env: BTreeMap<String, String> = [
            ("HOME", directory.display().to_string()),
            ("PATH", format!("{}:{inherited}", bin.display())),
            ("RUNNER_TEMP", directory.display().to_string()),
            (
                "GITHUB_STATE",
                directory.join("state").display().to_string(),
            ),
            ("GITHUB_ENV", directory.join("env").display().to_string()),
            (
                "TACT_CACHIX_TRACE",
                directory.join("trace").display().to_string(),
            ),
            ("TACT_FIXTURE_USER", username.clone()),
            ("INPUT_NAME", "public-fixture".into()),
            (
                "INPUT_AUTHTOKEN",
                if authenticated { "fixture-token" } else { "" }.into(),
            ),
            ("INPUT_SKIPPUSH", (!write).to_string()),
            ("INPUT_USEDAEMON", "true".into()),
            ("INPUT_SKIPADDINGSUBSTITUTER", "false".into()),
            ("INPUT_CACHIXBIN", bin.join("cachix").display().to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        for name in [
            "INPUT_SIGNINGKEY",
            "INPUT_EXTRAPULLNAMES",
            "INPUT_PATHSTOPUSH",
            "INPUT_PUSHFILTER",
            "INPUT_CACHIXARGS",
            "CACHIX_AUTH_TOKEN",
            "CACHIX_SIGNING_KEY",
            "NIX_CONF",
            "NIX_USER_CONF_FILES",
        ] {
            env.insert(name.into(), String::new());
        }
        if mode == "filtered" {
            env.insert(
                "INPUT_PUSHFILTER".into(),
                r"(-source$|\.tar\.gz$|\.zip$)".into(),
            );
        }
        let main = crate::process::run(
            Command::new("node").arg(&script).env_clear().envs(&env),
            root,
            30,
        )?;
        ensure!(main.code == Some(0), "Cachix {mode} main: {}", main.text);
        let state = crate::output::values(&fs::read_to_string(directory.join("state"))?)?;
        let exported = crate::output::values(&fs::read_to_string(directory.join("env"))?)?;
        ensure!(
            state.get("pushMode").map(String::as_str)
                == Some(if write { "Daemon" } else { "None" }),
            "unexpected push mode: {state:?}"
        );
        env.extend(exported.clone());
        env.extend(state.into_iter().map(|(k, v)| (format!("STATE_{k}"), v)));
        if write {
            let daemon = exported
                .get("CACHIX_DAEMON_DIR")
                .ok_or_else(|| anyhow::anyhow!("missing daemon directory"))?;
            let hook = crate::process::run(
                Command::new("bash")
                    .arg(Path::new(daemon).join("post-build-hook.sh"))
                    .env_clear()
                    .envs(&env)
                    .env("OUT_PATHS", "/nix/store/fixture-output /nix/store/fixture-source /nix/store/fixture.tar.gz /nix/store/fixture.zip"),
                root,
                30,
            )?;
            ensure!(hook.code == Some(0), "Cachix hook: {}", hook.text);
        }
        let post = crate::process::run(
            Command::new("node").arg(&script).env_clear().envs(&env),
            root,
            30,
        )?;
        ensure!(post.code == Some(0), "Cachix {mode} post: {}", post.text);
        let trace = fs::read_to_string(directory.join("trace"))?;
        ensure!(
            trace.contains("authtoken") == authenticated,
            "incorrect authentication mode: {trace}"
        );
        if mode == "filtered" {
            ensure!(
                trace.contains("fixture-output")
                    && !trace.contains("fixture-source")
                    && !trace.contains("fixture.tar.gz")
                    && !trace.contains("fixture.zip"),
                "filter did not exclude sources: {trace}"
            );
        }
        ensure!(
            trace.contains("use public-fixture"),
            "missing substituter setup"
        );
        if write {
            ensure!(
                ["authtoken", "daemon push", "daemon stop"]
                    .iter()
                    .all(|s| trace.contains(s)),
                "missing daemon lifecycle: {trace}"
            );
        } else {
            ensure!(
                !["daemon", "push"].iter().any(|s| trace.contains(s)),
                "read-only mode attempted a write: {trace}"
            );
            ensure!(
                authenticated || !main.text.contains("fixture-token"),
                "unauthenticated mode exposed token"
            );
        }
        println!("PASS Cachix {mode}: main and post lifecycle");
    }
    Ok(())
}
