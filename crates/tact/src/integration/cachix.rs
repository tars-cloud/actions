use anyhow::{Result, ensure};
use serde_json::json;
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
    for mode in ["read", "write", "fork"] {
        let directory = root.join(mode);
        fs::create_dir(&directory)?;
        for name in ["state", "env", "trace"] {
            fs::write(directory.join(name), "")?;
        }
        let config = json!({"cachix-name":"public-fixture","cachix-token":if mode=="read" {""} else {"fixture-token"}});
        let context = json!({"os":"Linux","arch":"X64","runner":"self-hosted","repository":"fixture/project","headRepository":if mode=="fork" {"fork/project"} else {""}});
        let policy=Command::new("node").args(["-e","process.stdout.write(require(process.argv[1]).policy(JSON.parse(process.argv[2]),JSON.parse(process.argv[3])).cachix)"])
            .arg(repository.join("composite/setup-cache/scripts/cache-plan/main.cjs")).arg(config.to_string()).arg(context.to_string()).output()?;
        ensure!(policy.status.success(), "Cachix policy failed");
        let write = policy.stdout == b"write";
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
                if write { "fixture-token" } else { "" }.into(),
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
                    .env("OUT_PATHS", "/nix/store/fixture-output"),
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
                !["authtoken", "daemon", "push"]
                    .iter()
                    .any(|s| trace.contains(s)),
                "read-only mode attempted a write: {trace}"
            );
            ensure!(
                !main.text.contains("fixture-token"),
                "read-only mode exposed token"
            );
        }
        println!("PASS Cachix {mode}: main and post lifecycle");
    }
    Ok(())
}
