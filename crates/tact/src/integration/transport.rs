use anyhow::{Result, ensure};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

struct Endpoint {
    url: String,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Endpoint {
    fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (requests, stopped) = (count.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut request = [0; 8192];
                    if stream.read(&mut request).is_ok() {
                        requests.fetch_add(1, Ordering::SeqCst);
                        let body = "<Error><Code>AccessDenied</Code><Message>Disposable denial fixture</Message></Error>";
                        let response = format!(
                            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });
        Ok(Self {
            url,
            count,
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn run(root: &Path) -> Result<()> {
    let endpoint = Endpoint::start()?;
    let cache = root.join("downloads");
    fs::create_dir(&cache)?;
    fs::write(cache.join("dependency"), "fixture")?;
    for file in ["output", "state"] {
        fs::write(root.join(file), "")?;
    }
    for phase in ["restore", "save"] {
        let script = super::download(
            root,
            &format!(
                "https://raw.githubusercontent.com/runs-on/cache/88d90644011a3a9957fd141a106f5a94f9794203/dist/{phase}/index.js"
            ),
            &format!("{phase}.cjs"),
        )?;
        let before = endpoint.count.load(Ordering::SeqCst);
        let result = crate::process::run(
            Command::new("node")
                .arg(script)
                .current_dir(root)
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .env("HOME", root)
                .env("GITHUB_REF", "refs/heads/test")
                .env("GITHUB_REPOSITORY", "fixture/actions")
                .env("GITHUB_WORKSPACE", root)
                .env("RUNNER_TEMP", root)
                .env("GITHUB_OUTPUT", root.join("output"))
                .env("GITHUB_STATE", root.join("state"))
                .env("INPUT_PATH", cache.as_os_str())
                .env("INPUT_KEY", "fixture-test-key")
                .env("INPUT_RESTORE-KEYS", "fixture-")
                .env("INPUT_ENABLECROSSOSARCHIVE", "false")
                .env("INPUT_LOOKUP-ONLY", "false")
                .env("INPUT_FAIL-ON-CACHE-MISS", "false")
                .env("RUNS_ON_S3_BUCKET_CACHE", "fixture")
                .env("RUNS_ON_S3_BUCKET_ENDPOINT", &endpoint.url)
                .env("RUNS_ON_S3_FORCE_PATH_STYLE", "true")
                .env("RUNS_ON_RUNNER_NAME", "")
                .env("RUNS_ON_AWS_REGION", "")
                .env("AWS_REGION", "us-east-1")
                .env("AWS_ACCESS_KEY_ID", "fixture-access-key")
                .env("AWS_SECRET_ACCESS_KEY", "fixture-secret-key")
                .env("AWS_EC2_METADATA_DISABLED", "true")
                .env("AWS_MAX_ATTEMPTS", "1"),
            root,
            30,
        )?;
        ensure!(result.code == Some(0), "S3 {phase}: {}", result.text);
        ensure!(
            endpoint.count.load(Ordering::SeqCst) > before,
            "S3 {phase} never reached fixture endpoint"
        );
        ensure!(
            result.text.contains("AccessDenied")
                || result.text.contains("Disposable denial fixture"),
            "missing denial diagnostic: {}",
            result.text
        );
        ensure!(
            phase != "save" || result.text.contains("::warning::"),
            "save must warn on failure"
        );
        ensure!(
            !result.text.contains("fixture-secret-key"),
            "secret leaked in transport diagnostics"
        );
        println!("PASS S3 {phase}: local denial remained nonfatal");
    }
    Ok(())
}
