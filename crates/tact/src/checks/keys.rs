use super::{
    CacheScenario,
    cache::{Fixture, different, s3, same},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::fs;

pub(super) fn run(f: &Fixture, scenario: &CacheScenario) -> Result<()> {
    f.write("Cargo.toml", "[package]")?;
    match scenario {
        CacheScenario::ContentKeys => {
            f.write("bun.lock", "a")?;
            let before = f.default_plan()?;
            f.write("bun.lock", "b")?;
            let after = f.default_plan()?;
            same(
                &after["caches"]["cargo"]["key"],
                &before["caches"]["cargo"]["key"],
                "independent Cargo key",
            )?;
            different(
                &after["caches"]["bun"]["key"],
                &before["caches"]["bun"]["key"],
                "changed Bun key",
            )?;
            f.write("scratch/Cargo.lock", "irrelevant")?;
            same(
                &f.default_plan()?["caches"]["cargo"]["key"],
                &before["caches"]["cargo"]["key"],
                "ignored lock",
            )?;
            f.write("Cargo.lock", "added")?;
            different(
                &f.default_plan()?["caches"]["cargo"]["key"],
                &before["caches"]["cargo"]["key"],
                "new lock",
            )?;
        }
        CacheScenario::EnvironmentKeys => {
            let direct = f.default_plan()?["caches"]["cargo"].clone();
            let flakes =
                f.plan(json!({"type":"flakes"}), json!({}), json!({}))?["caches"]["cargo"].clone();
            let named = f.plan(
                json!({"type":"flakes","flake-shell":".#named"}),
                json!({}),
                json!({}),
            )?;
            different(&direct["restore"], &flakes["restore"], "environment type")?;
            different(
                &named["caches"]["cargo"]["restore"],
                &flakes["restore"],
                "shell selector",
            )?;
            different(
                &f.plan(json!({}), json!({"arch":"ARM64"}), json!({}))?["caches"]["cargo"]["restore"],
                &direct["restore"],
                "architecture",
            )?;
            f.write("devenv.lock", r#"{"changed":true}"#)?;
            different(
                &f.default_plan()?["caches"]["cargo"]["restore"],
                &direct["restore"],
                "environment lock",
            )?;
            fs::remove_file(f.root().join("devenv.yaml"))?;
            same(
                &f.plan(json!({"type":"flakes"}), json!({}), json!({}))?["caches"]["cargo"]["key"],
                &flakes["key"],
                "flake independence",
            )?;
            ensure!(f.default_plan().is_err(), "missing devenv.yaml accepted");
        }
        CacheScenario::CompiledKeys => {
            let cfg = json!({"cargo-target":"true"});
            f.write("rust-toolchain.toml", "[toolchain]\nchannel=\"stable\"")?;
            let before = f.plan(cfg.clone(), json!({}), json!({}))?;
            for variant in [
                "cargo-build-variant",
                "cargo-build-target",
                "cargo-environment-key",
            ] {
                let mut config = cfg.clone();
                config[variant] = json!("different");
                let p = f.plan(config, json!({}), json!({}))?;
                different(
                    &p["caches"]["cargo-target"]["restore"],
                    &before["caches"]["cargo-target"]["restore"],
                    variant,
                )?;
                same(
                    &p["caches"]["cargo"]["key"],
                    &before["caches"]["cargo"]["key"],
                    "download independence",
                )?;
            }
            f.write("rust-toolchain.toml", "[toolchain]\nchannel=\"nightly\"")?;
            let toolchain = f.plan(cfg.clone(), json!({}), json!({}))?;
            different(
                &toolchain["caches"]["cargo-target"]["restore"],
                &before["caches"]["cargo-target"]["restore"],
                "compiler",
            )?;
            same(
                &toolchain["caches"]["cargo"]["key"],
                &before["caches"]["cargo"]["key"],
                "compiler download independence",
            )?;
            f.write(
                ".cargo/config.toml",
                "[build]\ntarget=\"aarch64-unknown-linux-gnu\"",
            )?;
            let config = f.plan(cfg.clone(), json!({}), json!({}))?;
            different(
                &config["caches"]["cargo-target"]["restore"],
                &toolchain["caches"]["cargo-target"]["restore"],
                "Cargo config",
            )?;
            same(
                &config["caches"]["cargo"]["key"],
                &before["caches"]["cargo"]["key"],
                "Cargo config download independence",
            )?;
            f.write(".cargo/registry/Cargo.toml", "[package]")?;
            same(
                &f.plan(cfg, json!({}), json!({}))?["caches"]["cargo"]["key"],
                &before["caches"]["cargo"]["key"],
                "registry excluded",
            )?;
        }
        CacheScenario::NixCompiledKeys => {
            for environment in ["devenv", "flakes"] {
                let config = json!({"type":environment,"cargo-target":"true"});
                for name in ["devenv.nix", "devenv.yaml", "flake.nix", "nix/compiler.nix"] {
                    f.write(name, "stable")?;
                    let before = f.plan(config.clone(), json!({}), json!({}))?;
                    f.write(name, "nightly")?;
                    let after = f.plan(config.clone(), json!({}), json!({}))?;
                    for field in ["key", "restore"] {
                        different(
                            &before["caches"]["cargo-target"][field],
                            &after["caches"]["cargo-target"][field],
                            &format!("{environment}: {name} compiled {field}"),
                        )?;
                    }
                    same(
                        &before["caches"]["cargo"],
                        &after["caches"]["cargo"],
                        "Nix changes preserve downloads",
                    )?;
                }
                let before = f.plan(config.clone(), json!({}), json!({}))?;
                f.write("nix/extra.nix", "{}")?;
                let added = f.plan(config.clone(), json!({}), json!({}))?;
                different(
                    &before["caches"]["cargo-target"]["restore"],
                    &added["caches"]["cargo-target"]["restore"],
                    "added module",
                )?;
                fs::remove_file(f.root().join("nix/extra.nix"))?;
                same(
                    &before["caches"],
                    &f.plan(config.clone(), json!({}), json!({}))?["caches"],
                    "removed module",
                )?;

                let mut excluded = config.clone();
                excluded["exclude"] = json!("*.nix\ndevenv.yaml");
                let before = f.plan(excluded.clone(), json!({}), json!({}))?;
                let required = if environment == "devenv" {
                    "devenv.nix"
                } else {
                    "flake.nix"
                };
                f.write(required, "changed despite exclusion")?;
                let after = f.plan(excluded.clone(), json!({}), json!({}))?;
                different(
                    &before["caches"]["cargo-target"]["restore"],
                    &after["caches"]["cargo-target"]["restore"],
                    "required definition cannot be excluded",
                )?;
                excluded["cargo-environment-key"] = json!("external-module-v2");
                let external = f.plan(excluded, json!({}), json!({}))?;
                different(
                    &after["caches"]["cargo-target"]["restore"],
                    &external["caches"]["cargo-target"]["restore"],
                    "external environment discriminator",
                )?;
                same(
                    &before["caches"]["cargo"],
                    &external["caches"]["cargo"],
                    "environment changes preserve downloads",
                )?;
            }
        }
        CacheScenario::TrustScopes => {
            let plan = |context| -> Result<Value> {
                Ok(f.plan(s3(), context, json!({}))?["caches"]["cargo"].clone())
            };
            let readable = plan(json!({"repository":"bingamon-lab/lz-cli"}))?;
            ensure!(
                readable["key"]
                    .as_str()
                    .unwrap()
                    .starts_with("bingamon-lab-lz-cli-cargo-Linux-X64-v1-"),
                "consumer-owned readable prefix"
            );
            same(
                &readable,
                &plan(json!({"repository":"BINGAMON-LAB/LZ-CLI"}))?,
                "repository case normalization",
            )?;
            let ambiguous = plan(json!({"repository":"bingamon/lab-lz-cli"}))?;
            ensure!(
                restore(&readable, &[ambiguous["key"].as_str().unwrap().to_string()]).is_none(),
                "ambiguous readable names must remain isolated"
            );
            ensure!(
                restore(&readable, &["tars-v1-old-format".into()]).is_none(),
                "legacy keys must not match"
            );
            let primary = plan(json!({"ref":"refs/heads/trunk"}))?;
            let pr = plan(json!({"pr":12,"headRepository":"example/project"}))?;
            let other = plan(json!({"pr":13,"headRepository":"example/project"}))?;
            let key = |p: &Value| p["key"].as_str().unwrap_or_default().to_string();
            let mut entries = vec![key(&primary)];
            ensure!(
                restore(&pr, &entries) == Some(key(&primary)),
                "PR must fall back to default branch"
            );
            entries.insert(0, key(&pr));
            ensure!(
                restore(&pr, &entries) == Some(key(&pr)),
                "PR must reuse its own saved cache"
            );
            ensure!(
                restore(&primary, &entries) == Some(key(&primary)),
                "default branch must not restore PR"
            );
            entries.pop();
            ensure!(
                restore(&primary, &entries).is_none() && restore(&other, &entries).is_none(),
                "trust scopes leaked"
            );
            different(
                &plan(json!({"ref":"refs/heads/other"}))?["key"],
                &plan(json!({}))?["key"],
                "branch",
            )?;
            different(
                &plan(json!({"repository":"other/project"}))?["restore"],
                &pr["restore"],
                "repository",
            )?;
        }
        CacheScenario::ConcurrentKeys => {
            same(
                &f.default_plan()?["caches"]["cargo"]["key"],
                &f.default_plan()?["caches"]["cargo"]["key"],
                "identical concurrent writers",
            )?;
            let key = |variant| -> Result<Value> {
                Ok(f.plan(
                    json!({"cargo-target":"true","cargo-build-variant":variant}),
                    json!({}),
                    json!({}),
                )?["caches"]["cargo-target"]["key"]
                    .clone())
            };
            different(&key("one")?, &key("two")?, "build variants")?;
        }
        _ => anyhow::bail!("not a key scenario"),
    }
    Ok(())
}

fn restore(cache: &Value, entries: &[String]) -> Option<String> {
    std::iter::once(cache["key"].as_str().unwrap_or_default())
        .chain(cache["restore"].as_str().unwrap_or_default().lines())
        .find_map(|prefix| entries.iter().find(|key| key.starts_with(prefix)).cloned())
}
