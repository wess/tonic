#[path = "support/process.rs"]
mod process;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn copy_project(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if matches!(name.to_str(), Some("_build" | "deps")) || name.to_string_lossy().ends_with(".db") {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_project(&path, &to.join(&name));
        } else {
            fs::copy(&path, to.join(&name)).unwrap();
        }
    }
}

fn success(result: &Output) {
    assert!(
        result.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

/// `curl` the server: (body, status).
fn request(port: u16, args: &[&str], path: &str) -> (String, String) {
    let mut command = Command::new("curl");
    command.args(["-s", "-w", "\n%{http_code}"]).args(args).arg(format!("http://127.0.0.1:{port}{path}"));
    let out = process::execute(command, Duration::from_secs(30));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    (body.trim_end().to_string(), status.to_string())
}

/// The example web service (Plug, Bandit, Ecto, SQLite with a C NIF) built
/// natively from fetched sources, serving its documented endpoints.
#[test]
#[ignore]
fn helloweb_example_serves_its_endpoints() {
    assert_eq!(
        std::env::var("TONIC_TEST_HEX").as_deref(),
        Ok("1"),
        "set TONIC_TEST_HEX=1 to authorize the network package test"
    );
    let root: PathBuf = std::env::temp_dir().join(format!("tonichelloweb{}", std::process::id()));
    let project = root.join("helloweb");
    copy_project(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/helloweb"), &project);
    let mix = |args: &[&str]| {
        let mut command = Command::new("mix");
        command
            .current_dir(&project)
            .env("MIX_HOME", root.join("mixhome"))
            .env("HEX_HOME", root.join("hexhome"))
            .env("MIX_ENV", "dev")
            .args(args);
        process::execute(command, Duration::from_secs(600))
    };
    success(&mix(&["local.hex", "--force"]));
    success(&mix(&["deps.get"]));
    let mut build = Command::new(env!("CARGO_BIN_EXE_tonic"));
    build
        .current_dir(&project)
        .env("MIX_HOME", root.join("mixhome"))
        .env("HEX_HOME", root.join("hexhome"))
        .env("HEX_OFFLINE", "1")
        .env("MIX_ENV", "dev")
        .args(["build", ".", "-o", "helloweb"]);
    success(&process::execute(build, Duration::from_secs(1800)));

    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut server = Command::new(project.join("helloweb"))
        .current_dir(&root)
        .env("PORT", port.to_string())
        .env("DATABASE", root.join("test.db"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while request(port, &[], "/").1 != "200" {
        assert!(Instant::now() < deadline, "server did not start");
        assert!(server.try_wait().unwrap().is_none(), "server exited during startup");
        std::thread::sleep(Duration::from_millis(250));
    }
    let json = ["-H", "content-type: application/json", "-X", "POST", "-d"];
    assert_eq!(request(port, &[], "/"), ("Hello, web!".into(), "200".into()));
    let (created, status) = request(port, &[&json[..], &[r#"{"name":"world"}"#]].concat(), "/greetings");
    assert_eq!(status, "201");
    assert!(created.contains(r#""id":1"#) && created.contains(r#""name":"world""#), "{created}");
    assert_eq!(
        request(port, &[&json[..], &[r#"{"name":""}"#]].concat(), "/greetings"),
        (r#"{"errors":{"name":["can't be blank"]}}"#.into(), "422".into())
    );
    assert_eq!(request(port, &[&json[..], &["{bad"]].concat(), "/greetings").1, "400");
    let (all, status) = request(port, &[], "/greetings");
    assert_eq!(status, "200");
    assert!(all.starts_with("[{") && all.contains(r#""name":"world""#), "{all}");
    assert_eq!(request(port, &[], "/greetings/1").1, "200");
    assert_eq!(request(port, &[], "/greetings/999"), (r#"{"error":"not found"}"#.into(), "404".into()));
    assert_eq!(request(port, &[], "/nope").1, "404");
    // concurrent writers
    let handles: Vec<_> = (0..20)
        .map(|i| {
            std::thread::spawn(move || {
                let body = format!(r#"{{"name":"n{i}"}}"#);
                request(port, &["-H", "content-type: application/json", "-X", "POST", "-d", &body], "/greetings").1
            })
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), "201");
    }
    let _ = server.kill();
    let _ = server.wait();
    let _ = fs::remove_dir_all(root);
}
