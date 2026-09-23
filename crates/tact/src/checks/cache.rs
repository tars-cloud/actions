use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, ensure};
use serde_json::{Value, json};

use super::{CacheScenario, keys};

pub(super) struct Fixture {
    directory: tempfile::TempDir,
    script: PathBuf,
}

impl Fixture {
    pub fn new(root: &Path) -> Result<Self> {
        let scratch = root.join(".tars/scratch/cache");
        fs::create_dir_all(&scratch)?;
        let fixture = Self {
            directory: tempfile::tempdir_in(scratch)?,
            script: root.join("composite/setup-cache/scripts/cache-plan/main.cjs"),
        };
        for path in [
            "devenv.nix",
            "devenv.yaml",
            "devenv.lock",
            "flake.nix",
            "flake.lock",
        ] {
            fixture.write(path, "{}")?;
        }
        Ok(fixture)
    }
    pub fn root(&self) -> &Path {
        self.directory.path()
    }
    pub fn write(&self, path: &str, content: &str) -> Result<()> {
        let path = self.root().join(path);
        fs::create_dir_all(path.parent().unwrap_or(self.root()))?;
        Ok(fs::write(path, content)?)
    }
    pub fn plan(&self, config: Value, changes: Value, env: Value) -> Result<Value> {
        let mut context = json!({"runner":"self-hosted", "os":"Linux", "arch":"X64", "repository":"example/project", "defaultBranch":"trunk", "ref":"refs/heads/topic", "workspace":self.root()});
        merge(&mut context, changes);
        let mut environment = json!({"HOME":self.root().join("home")});
        merge(&mut environment, env);
        let input = json!({"config":config,"context":context,"env":environment});
        // Call the production API; assertions and fixtures remain in Rust.
        let driver = "const a=JSON.parse(process.argv[1]); const p=require(process.argv[2]).cachePlan(a.config,a.context,a.env,new Date('2026-09-22T12:00:00Z')); process.stdout.write(JSON.stringify(p));";
        let result = Command::new("node")
            .args(["-e", driver, &input.to_string()])
            .arg(&self.script)
            .output()?;
        ensure!(
            result.status.success(),
            "cache plan failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(serde_json::from_slice(&result.stdout)?)
    }
    pub fn default_plan(&self) -> Result<Value> {
        self.plan(json!({}), json!({}), json!({}))
    }
}

fn merge(target: &mut Value, source: Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        target.extend(source.clone());
    }
}

pub(super) fn s3() -> Value {
    json!({"s3-endpoint":"https://cache.example.invalid","s3-bucket":"fixture","s3-region":"us-east-1","s3-access-key":"fixture-key","s3-secret-key":"fixture-secret"})
}
pub(super) fn same(actual: &Value, expected: &Value, label: &str) -> Result<()> {
    ensure!(
        actual == expected,
        "{label}: expected {expected}, actual {actual}"
    );
    Ok(())
}
pub(super) fn different(a: &Value, b: &Value, label: &str) -> Result<()> {
    ensure!(a != b, "{label}: expected different values, both were {a}");
    Ok(())
}

pub(super) fn run(root: &Path, scenario: &CacheScenario) -> Result<()> {
    let f = Fixture::new(root)?;
    match scenario {
        CacheScenario::Backend => {
            same(
                &f.default_plan()?["backend"],
                &json!("github"),
                "unconfigured self-hosted",
            )?;
            same(
                &f.plan(s3(), json!({}), json!({}))?["backend"],
                &json!("s3"),
                "explicit S3",
            )?;
            same(
                &f.plan(
                    json!({"s3-bucket":"unused"}),
                    json!({"runner":"github-hosted"}),
                    json!({}),
                )?["backend"],
                &json!("github"),
                "hosted ignores S3",
            )?;
            let error = f
                .plan(json!({"s3-bucket":"fixture"}), json!({}), json!({}))
                .expect_err("partial S3 must fail")
                .to_string();
            ensure!(
                error.contains("s3-endpoint, s3-region, s3-access-key, s3-secret-key"),
                "incomplete S3 diagnostic: {error}"
            );
            let error = f
                .plan(
                    json!({"s3-session-token":"secret-value"}),
                    json!({}),
                    json!({}),
                )
                .expect_err("partial S3 must fail")
                .to_string();
            ensure!(!error.contains("secret-value"), "credentials leaked");
        }
        CacheScenario::Fork => {
            for runner in ["github-hosted", "self-hosted"] {
                let p = f.plan(
                    json!({"s3-bucket":"unused","cachix-name":"public","cachix-token":"secret"}),
                    json!({"runner":runner,"headRepository":"fork/project","pr":7}),
                    json!({}),
                )?;
                same(&p["backend"], &json!("github"), "fork backend")?;
                same(&p["cachix"], &json!("read"), "fork Cachix")?;
                same(&p["fork"], &json!(true), "fork detection")?;
            }
            for (cfg, expected) in [
                (json!({"cachix-token":"secret"}), "disabled"),
                (json!({"cachix-name":"public"}), "read"),
                (
                    json!({"cachix-name":"public","cachix-token":"secret"}),
                    "write",
                ),
            ] {
                same(
                    &f.plan(
                        cfg,
                        json!({"headRepository":"example/project","pr":7}),
                        json!({}),
                    )?["cachix"],
                    &json!(expected),
                    "same-repo Cachix",
                )?;
            }
        }
        CacheScenario::Platform => {
            for changes in [
                json!({"os":"Windows"}),
                json!({"os":"macOS"}),
                json!({"arch":"ARM"}),
                json!({"runner":"unknown"}),
            ] {
                ensure!(
                    f.plan(json!({}), changes, json!({})).is_err(),
                    "unsupported platform accepted"
                );
            }
            same(
                &f.plan(json!({}), json!({"arch":"ARM64"}), json!({}))?["backend"],
                &json!("github"),
                "ARM64",
            )?;
        }
        CacheScenario::Discovery => {
            for path in [
                "rust/Cargo.toml",
                "python/uv.lock",
                "python/pyproject.toml",
                "legacy/requirements.txt",
                "frontend/bun.lockb",
                "security/trivy.yaml",
                "scratch/ignored/bun.lock",
                "node_modules/fake/Cargo.toml",
            ] {
                f.write(path, "fixture")?;
            }
            std::os::unix::fs::symlink("/tmp", f.root().join("outside"))?;
            let p = f.default_plan()?;
            same(
                &p["tools"],
                &json!(["cargo", "bun", "trivy", "uv", "pip"]),
                "mixed discovery",
            )?;
            let cargo = p["caches"]["cargo"]["path"].as_str().unwrap_or_default();
            ensure!(
                !cargo.contains("/target") && !cargo.contains("/registry/src"),
                "compiled files in download cache"
            );
            same(
                &f.plan(
                    json!({"exclude":"rust\nfrontend\npython\nlegacy\nsecurity"}),
                    json!({}),
                    json!({}),
                )?["tools"],
                &json!([]),
                "explicit exclusions",
            )?;
            let other = Fixture::new(root)?;
            for path in [
                ".github/workflows/trivy.yml",
                "nested/.github/workflows/trivy.yaml",
            ] {
                other.write(path, "{}")?;
            }
            same(
                &other.default_plan()?["tools"],
                &json!([]),
                "workflow exclusions",
            )?;
        }
        CacheScenario::Selection => {
            f.write("package.json", "{}")?;
            f.write("pyproject.toml", "[project]")?;
            let p = f.default_plan()?;
            same(&p["tools"], &json!([]), "ambiguous metadata")?;
            ensure!(
                p["reasons"].as_array().is_some_and(|r| r.len() == 2),
                "missing ambiguity notices"
            );
            same(
                &f.plan(
                    json!({"tools":"bun python trivy","python-manager":"uv"}),
                    json!({}),
                    json!({}),
                )?["tools"],
                &json!(["bun", "trivy", "uv"]),
                "explicit tools",
            )?;
            f.write("package.json", r#"{"packageManager":"bun@1.3.0"}"#)?;
            same(
                &f.default_plan()?["tools"],
                &json!(["bun"]),
                "Bun packageManager",
            )?;
            for cfg in [
                json!({"tools":"none"}),
                json!({"tools":"none","trivy-cache-path":"/"}),
            ] {
                same(
                    &f.plan(cfg, json!({}), json!({}))?["tools"],
                    &json!([]),
                    "none",
                )?;
            }
            for cfg in [json!({"tools":"node"}), json!({"python-manager":"poetry"})] {
                ensure!(
                    f.plan(cfg, json!({}), json!({})).is_err(),
                    "invalid selection accepted"
                );
            }
        }
        CacheScenario::Paths => {
            let p=f.plan(json!({"tools":"cargo python bun trivy","python-manager":"uv","cargo-target":"true","cargo-cache-path":"custom/cargo","uv-cache-path":"custom/uv"}),json!({}),json!({"CARGO_HOME":"/unused","UV_CACHE_DIR":"/unused","BUN_INSTALL_CACHE_DIR":"/bun-cache","XDG_CACHE_HOME":"/xdg","CARGO_TARGET_DIR":"out"}))?;
            let cargo = ["registry/index", "registry/cache", "git/db"]
                .map(|s| f.root().join("custom/cargo").join(s).display().to_string())
                .join("\n");
            same(&p["caches"]["cargo"]["path"], &json!(cargo), "Cargo paths")?;
            for (tool, path) in [
                ("uv", f.root().join("custom/uv")),
                ("bun", PathBuf::from("/bun-cache")),
                ("trivy", PathBuf::from("/xdg/trivy")),
                ("cargo-target", f.root().join("out")),
            ] {
                same(&p["caches"][tool]["path"], &json!(path), tool)?;
            }
            same(
                &p["exports"]["CARGO_HOME"],
                &json!(f.root().join("custom/cargo")),
                "Cargo export",
            )?;
            for path in ["/\nINJECT=yes", "/"] {
                ensure!(
                    f.plan(
                        json!({"tools":"trivy","trivy-cache-path":path}),
                        json!({}),
                        json!({})
                    )
                    .is_err(),
                    "invalid cache directory accepted"
                );
            }
        }
        _ => keys::run(&f, scenario)?,
    }
    Ok(())
}
