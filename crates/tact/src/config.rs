use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub version: u32,
    pub mirrors: Option<Mirrors>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Mirrors {
    pub retention_days: u64,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    pub packages: Vec<Package>,
}

fn timeout() -> u64 {
    300
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Package {
    pub name: String,
    pub kind: String,
    pub destination: String,
    pub tags: Vec<String>,
    pub sources: Vec<String>,
}

pub(crate) fn load(path: &Path) -> Result<Config> {
    let value: serde_json::Value = serde_norway::from_str(&fs::read_to_string(path)?)
        .with_context(|| format!("parse {}", path.display()))?;
    let schema = serde_json::from_str(include_str!("../../../schemas/tact.schema.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let errors: Vec<_> = validator
        .iter_errors(&value)
        .map(|error| {
            format!(
                "{}: does not match the Tact configuration schema",
                error.instance_path()
            )
        })
        .collect();
    ensure!(
        errors.is_empty(),
        "{}: {}",
        path.display(),
        errors.join("; ")
    );
    let config: Config = serde_json::from_value(value)?;
    ensure!(
        config.version == 1,
        "unsupported Tact configuration version"
    );
    if let Some(mirrors) = &config.mirrors {
        let mut names = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        for package in &mirrors.packages {
            ensure!(package.kind == "oci", "unsupported package kind");
            ensure!(
                names.insert(&package.name),
                "duplicate mirror name: {}",
                package.name
            );
            ensure!(
                destinations.insert(&package.destination),
                "duplicate mirror destination: {}",
                package.destination
            );
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn config() -> Value {
        json!({"version": 1, "mirrors": {"retention-days": 3, "packages": [{
            "name":"example", "kind":"oci", "destination":"ghcr.io/example/actions/database",
            "tags":["2","daily"], "sources":["ghcr.io/upstream/database:2"]
        }]}})
    }

    fn check(value: &Value) -> Result<Config> {
        let scratch =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tars/scratch/config-tests");
        fs::create_dir_all(&scratch)?;
        let work = tempfile::tempdir_in(scratch)?;
        let path = work.path().join("tact.yaml");
        fs::write(&path, format!("---\n{}", serde_json::to_string(value)?))?;
        load(&path)
    }

    #[test]
    fn versioned_sections_are_optional_and_mirror_defaults_apply() -> Result<()> {
        assert!(check(&json!({"version":1}))?.mirrors.is_none());
        assert_eq!(check(&config())?.mirrors.unwrap().timeout_seconds, 300);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        load(&root.join("tact.yaml"))?;
        Ok(())
    }

    #[test]
    fn rejects_typos_unsupported_versions_empty_lists_and_unsafe_references() {
        for (pointer, value) in [
            ("/version", json!(2)),
            ("/mirrors/retention-days", json!(0)),
            ("/mirrors/retention-days", json!(366)),
            ("/mirrors/timeout-seconds", json!(0)),
            ("/mirrors/packages", json!([])),
            ("/mirrors/packages/0/kind", json!("http")),
            ("/mirrors/packages/0/name", json!("../escape")),
            (
                "/mirrors/packages/0/destination",
                json!("ghcr.io/example/actions/db:2"),
            ),
            (
                "/mirrors/packages/0/destination",
                json!("ghcr.io/example/../database"),
            ),
            ("/mirrors/packages/0/sources", json!(["--help"])),
            (
                "/mirrors/packages/0/sources",
                json!(["https://user:password@registry.example/db:2"]),
            ),
            ("/mirrors/packages/0/sources", json!(["ghcr.io/example/db"])),
            ("/mirrors/packages/0/sources", json!([])),
            ("/mirrors/packages/0/tags", json!(["daily", "daily"])),
            ("/mirrors/packages/0/tags", json!(["tact-staging"])),
            ("/mirrors/packages/0/tags", json!(["-danger"])),
        ] {
            let mut config = config();
            if pointer == "/mirrors/timeout-seconds" {
                config["mirrors"]["timeout-seconds"] = value;
            } else {
                *config.pointer_mut(pointer).unwrap() = value;
            }
            assert!(check(&config).is_err(), "{pointer}: {config}");
        }
        for pointer in ["", "/mirrors", "/mirrors/packages/0"] {
            let mut config = config();
            config
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("typo".into(), json!(true));
            assert!(check(&config).is_err());
        }
    }

    #[test]
    fn rejects_duplicate_names_and_destinations_but_accepts_digest_sources() -> Result<()> {
        let mut value = config();
        let package = value["mirrors"]["packages"][0].clone();
        value["mirrors"]["packages"]
            .as_array_mut()
            .unwrap()
            .push(package);
        assert!(
            check(&value)
                .unwrap_err()
                .to_string()
                .contains("duplicate mirror name")
        );
        value["mirrors"]["packages"][1]["name"] = json!("other");
        assert!(
            check(&value)
                .unwrap_err()
                .to_string()
                .contains("duplicate mirror destination")
        );
        let mut value = config();
        value["mirrors"]["packages"][0]["sources"] =
            json!([format!("ghcr.io/upstream/db@sha256:{}", "a".repeat(64))]);
        check(&value)?;
        Ok(())
    }

    #[test]
    fn validation_errors_do_not_echo_credential_bearing_values() {
        let mut value = config();
        value["mirrors"]["packages"][0]["sources"] =
            json!(["https://user:fixture-credential@registry.example/db:2"]);
        let error = check(&value).unwrap_err().to_string();
        assert!(error.contains("/mirrors/packages/0/sources/0"));
        assert!(!error.contains("fixture-credential"));
    }
}
