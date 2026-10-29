use super::*;
use serde_json::json;
use std::process::Command;

const CURRENT: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PREVIOUS: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const OLD: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const NOW: u64 = 1_800_000_000;

fn package() -> Package {
    Package {
        name: "sample".into(),
        kind: "oci".into(),
        destination: "ghcr.io/example/actions/sample".into(),
        tags: vec!["2".into(), "daily".into()],
        sources: vec![
            "ghcr.io/upstream/sample:2".into(),
            "docker.io/upstream/sample:2".into(),
            "public.ecr.aws/upstream/sample:2".into(),
        ],
    }
}

#[derive(Default)]
struct Fake {
    calls: Vec<(String, Vec<String>)>,
    failed_sources: Vec<&'static str>,
    fail: Option<&'static str>,
    mismatch: Option<&'static str>,
    versions: usize,
}

impl Tools for Fake {
    fn run(&mut self, program: &str, args: &[String]) -> Result<String> {
        self.calls.push((program.into(), args.to_vec()));
        if program == "gh" {
            if args.iter().any(|arg| arg == "DELETE") {
                ensure!(self.fail != Some("delete"), "delete failed");
                return Ok(String::new());
            }
            ensure!(self.fail != Some("versions"), "versions unavailable");
            self.versions += 1;
            let tags = if self.versions == 1 {
                vec!["2".to_owned(), "daily".to_owned()]
            } else {
                vec![format!("tact-retained-{NOW}-1")]
            };
            return Ok(json!([
                {"id":1,"digest":PREVIOUS,"created":1,"tags":tags},
                {"id":2,"digest":CURRENT,"created":NOW,"tags":["tact-staging","2","daily"]},
                {"id":3,"digest":OLD,"created":1,"tags":["tact-retained-2-3"]}
            ])
            .to_string());
        }
        ensure!(program == "oras", "unexpected program");
        match args[0].as_str() {
            "resolve" => {
                let reference = &args[1];
                ensure!(
                    !self
                        .failed_sources
                        .iter()
                        .any(|source| reference.starts_with(source)),
                    "registry unavailable"
                );
                let phase = if reference.ends_with(":artifact") {
                    "local"
                } else if reference.ends_with(":tact-staging") {
                    "staging"
                } else if reference.ends_with(":daily") {
                    "daily"
                } else {
                    "source"
                };
                Ok(if self.mismatch == Some(phase) {
                    OLD
                } else {
                    CURRENT
                }
                .into())
            }
            "cp" => {
                let phase = if args.iter().any(|arg| arg == "--to-oci-layout") {
                    "download"
                } else {
                    "upload"
                };
                ensure!(self.fail != Some(phase), "copy failed");
                Ok(String::new())
            }
            "tag" => {
                let phase = if args.iter().any(|arg| arg.starts_with(retention::RETAINED)) {
                    "retain"
                } else {
                    "promote"
                };
                ensure!(self.fail != Some(phase), "tag failed");
                Ok(String::new())
            }
            _ => bail!("unexpected ORAS command"),
        }
    }
}

fn scratch() -> tempfile::TempDir {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tars/scratch/mirror-tests");
    fs::create_dir_all(&path).unwrap();
    tempfile::tempdir_in(path).unwrap()
}

#[test]
fn falls_back_in_order_downloads_by_digest_and_retires_before_promotion() -> Result<()> {
    let mut tools = Fake {
        failed_sources: vec!["ghcr.io/upstream", "docker.io/upstream"],
        ..Fake::default()
    };
    sync(&mut tools, &package(), scratch().path(), NOW, 3)?;
    let resolves: Vec<_> = tools
        .calls
        .iter()
        .filter(|(_, args)| args[0] == "resolve")
        .map(|(_, args)| args[1].as_str())
        .collect();
    assert_eq!(&resolves[..3], package().sources);
    let copies: Vec<_> = tools
        .calls
        .iter()
        .filter(|(_, args)| args[0] == "cp")
        .collect();
    assert!(
        copies[0]
            .1
            .contains(&format!("public.ecr.aws/upstream/sample@{CURRENT}"))
    );
    let tags: Vec<_> = tools
        .calls
        .iter()
        .filter(|(_, args)| args[0] == "tag")
        .collect();
    assert_eq!(
        tags[0].1,
        [
            "tag",
            &format!("ghcr.io/example/actions/sample@{PREVIOUS}"),
            &format!("tact-retained-{NOW}-1")
        ]
    );
    assert!(tags[1].1.ends_with(&["2".into(), "daily".into()]));
    let deletes: Vec<_> = tools
        .calls
        .iter()
        .filter(|(_, args)| args.iter().any(|arg| arg == "DELETE"))
        .collect();
    assert_eq!(deletes.len(), 1);
    assert_eq!(
        deletes[0].1.last().unwrap(),
        "/orgs/example/packages/container/actions%2Fsample/versions/3"
    );
    Ok(())
}

#[test]
fn first_success_stops_upstream_requests() -> Result<()> {
    let mut tools = Fake::default();
    sync(&mut tools, &package(), scratch().path(), NOW, 3)?;
    assert!(!tools.calls.iter().any(|(_, args)| {
        args.iter()
            .any(|arg| arg.contains("docker.io") || arg.contains("public.ecr.aws"))
    }));
    Ok(())
}

#[test]
fn source_failures_never_write_to_ghcr_and_failed_downloads_try_every_source() {
    for mut tools in [
        Fake {
            failed_sources: vec![
                "ghcr.io/upstream",
                "docker.io/upstream",
                "public.ecr.aws/upstream",
            ],
            ..Fake::default()
        },
        Fake {
            fail: Some("download"),
            ..Fake::default()
        },
        Fake {
            mismatch: Some("local"),
            ..Fake::default()
        },
    ] {
        let error = sync(&mut tools, &package(), scratch().path(), NOW, 3).unwrap_err();
        assert!(error.to_string().contains("all sources failed"));
        assert!(tools.calls.iter().all(|(program, args)| program == "oras"
            && !["tag"].contains(&args[0].as_str())
            && !args.iter().any(|arg| arg == "--from-oci-layout")));
        assert_eq!(
            tools
                .calls
                .iter()
                .filter(|(_, args)| args[0] == "resolve" && package().sources.contains(&args[1]))
                .count(),
            3
        );
    }
}

#[test]
fn publication_failures_never_prune_or_retry_another_source() {
    for phase in ["upload", "versions", "retain", "promote"] {
        let mut tools = Fake {
            fail: Some(phase),
            ..Fake::default()
        };
        assert!(
            sync(&mut tools, &package(), scratch().path(), NOW, 3).is_err(),
            "{phase}"
        );
        assert!(!tools.calls.iter().any(|(_, args)| {
            args.iter()
                .any(|arg| arg == "DELETE" || arg.contains("docker.io"))
        }));
    }
    for phase in ["staging", "daily"] {
        let mut tools = Fake {
            mismatch: Some(phase),
            ..Fake::default()
        };
        assert!(sync(&mut tools, &package(), scratch().path(), NOW, 3).is_err());
        assert!(
            !tools
                .calls
                .iter()
                .any(|(_, args)| args.iter().any(|arg| arg == "DELETE"))
        );
    }
}

#[test]
fn pruning_errors_fail_the_refresh() {
    let mut tools = Fake {
        fail: Some("delete"),
        ..Fake::default()
    };
    assert!(
        sync(&mut tools, &package(), scratch().path(), NOW, 3)
            .unwrap_err()
            .to_string()
            .contains("delete failed")
    );
}

#[test]
fn failed_packages_do_not_prevent_other_packages_from_refreshing() {
    let mut broken = package();
    broken.name = "broken".into();
    broken.sources = vec!["ghcr.io/unavailable/db:2".into()];
    let mirrors = Mirrors {
        retention_days: 3,
        timeout_seconds: 1,
        packages: vec![broken, package()],
    };
    let mut tools = Fake {
        failed_sources: vec!["ghcr.io/unavailable"],
        ..Fake::default()
    };
    let error = refresh(&mut tools, &mirrors, scratch().path()).unwrap_err();
    assert_eq!(error.to_string(), "mirror refresh failed: broken");
    assert!(
        tools
            .calls
            .iter()
            .any(|(_, args)| args[0] == "tag" && args.last().is_some_and(|tag| tag == "daily"))
    );
}

#[test]
fn reference_pinning_and_digest_validation() -> Result<()> {
    for source in [
        "ghcr.io/upstream/sample:2",
        &format!("ghcr.io/upstream/sample@{OLD}"),
    ] {
        assert_eq!(
            pinned(source, CURRENT),
            format!("ghcr.io/upstream/sample@{CURRENT}")
        );
    }
    for invalid in [
        "",
        "sha256:abc",
        "sha512:abc",
        &format!("sha256:{}", "g".repeat(64)),
    ] {
        assert!(digest(invalid).is_err());
    }
    assert_eq!(digest(CURRENT)?, CURRENT);
    Ok(())
}

#[test]
fn real_oras_preserves_custom_media_types_and_manifest_bytes_in_local_layouts() -> Result<()> {
    let work = scratch();
    fs::write(work.path().join("database"), "fixture database")?;
    let execute = |args: &[&str]| -> Result<String> {
        let output = Command::new(crate::runner::executable("oras")?)
            .args(args)
            .current_dir(work.path())
            .output()?;
        ensure!(
            output.status.success(),
            "ORAS local fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?)
    };
    execute(&[
        "push",
        "--oci-layout",
        "source:artifact",
        "--artifact-type",
        "application/vnd.aquasec.trivy.config.v1+json",
        "database:application/vnd.aquasec.trivy.db.layer.v1.tar+gzip",
    ])?;
    execute(&[
        "cp",
        "--from-oci-layout",
        "--to-oci-layout",
        "source:artifact",
        "target:artifact",
    ])?;
    assert_eq!(
        execute(&["manifest", "fetch", "--oci-layout", "source:artifact"])?,
        execute(&["manifest", "fetch", "--oci-layout", "target:artifact"])?
    );
    assert_eq!(
        execute(&["resolve", "--oci-layout", "source:artifact"])?,
        execute(&["resolve", "--oci-layout", "target:artifact"])?
    );
    Ok(())
}
