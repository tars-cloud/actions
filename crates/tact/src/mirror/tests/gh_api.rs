use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

struct LocalGh {
    url: String,
    scratch: tempfile::TempDir,
}

impl Tools for LocalGh {
    fn run(&mut self, program: &str, args: &[String]) -> Result<String> {
        assert_eq!(program, "gh");
        assert_eq!(args[0], "api");
        assert_eq!(
            args[1],
            "/orgs/example/packages/container/actions%2Fsample/versions?per_page=100"
        );
        let mut args = args.to_vec();
        args[1] = format!("{}/versions?per_page=100", self.url);
        let mut command = Command::new(crate::runner::executable("gh")?);
        command
            .args(args)
            .env_clear()
            .env("HOME", self.scratch.path())
            .env("GH_CONFIG_DIR", self.scratch.path())
            .env("GH_TOKEN", "fixture-token")
            .env("GH_PROMPT_DISABLED", "1");
        let result = crate::process::run(&mut command, self.scratch.path(), 10)?;
        ensure!(result.code == Some(0), "gh API fixture: {}", result.text);
        Ok(result.text)
    }
}

#[test]
fn real_gh_reads_all_package_version_pages_and_converts_timestamps() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let url = format!("http://{}", listener.local_addr()?);
    let mut tools = LocalGh {
        url: url.clone(),
        scratch: scratch(),
    };
    let server = std::thread::spawn(move || -> Result<()> {
        for (index, digest) in [CURRENT, OLD].iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        ensure!(Instant::now() < deadline, "missing API page {index}");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte)?;
                request.push(byte[0]);
                ensure!(request.len() < 8192, "oversized fixture request");
            }
            let request = String::from_utf8(request)?;
            let path = if index == 0 {
                "/versions?per_page=100"
            } else {
                "/versions?per_page=100&page=2"
            };
            ensure!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            let body = json!([{
                "id": index + 1,
                "name": digest,
                "created_at": "2026-10-29T06:00:00Z",
                "metadata": {"container": {"tags": if index == 0 {vec!["2", "daily"]} else {vec![]}}}
            }]).to_string();
            let link = if index == 0 {
                format!("Link: <{url}/versions?per_page=100&page=2>; rel=\"next\"\r\n")
            } else {
                String::new()
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{link}Connection: close\r\n\r\n{body}",
                body.len()
            )?;
        }
        Ok(())
    });
    let result = super::super::retention::versions(&mut tools, &package());
    let served = server.join().expect("API fixture thread");
    let versions = result?;
    served?;
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].id, 1);
    assert_eq!(versions[0].digest, CURRENT);
    assert_eq!(versions[0].created, 1_793_253_600);
    assert_eq!(versions[0].tags, ["2", "daily"]);
    assert_eq!(versions[1].id, 2);
    assert_eq!(versions[1].digest, OLD);
    assert!(versions[1].tags.is_empty());
    Ok(())
}

#[test]
fn invalid_or_incomplete_version_pages_fail_instead_of_returning_partial_history() {
    struct Response(String);
    impl Tools for Response {
        fn run(&mut self, _: &str, _: &[String]) -> Result<String> {
            Ok(self.0.clone())
        }
    }
    let valid = json!([{"id":1,"digest":CURRENT,"created":1,"tags":[]}]).to_string();
    for response in [
        String::new(),
        "[]\n[]".into(),
        format!("{valid}\n["),
        format!("{valid}\n{}", json!([{"id":2,"digest":OLD}])),
        format!(
            "{valid}\n{}",
            json!([{"id":2,"digest":"invalid","created":1,"tags":[]}])
        ),
    ] {
        assert!(super::super::retention::versions(&mut Response(response), &package()).is_err());
    }
}
