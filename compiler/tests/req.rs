#[path = "support/process.rs"]
mod process;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

struct Server(std::process::Child);

impl Drop for Server {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

fn copy(from: &Path, to: &Path) {
  fs::create_dir_all(to).unwrap();
  for entry in fs::read_dir(from).unwrap() {
    let entry = entry.unwrap();
    if entry.file_name() == "deps" || entry.file_name() == "_build" {
      continue;
    }
    if entry.file_type().unwrap().is_dir() {
      copy(&entry.path(), &to.join(entry.file_name()));
    } else {
      fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
  }
}

#[test]
#[ignore = "requires pinned Hex downloads; set TONIC_TEST_HEX=1 and pass --ignored"]
fn unchanged_req_get_post_redirect_and_status_match_beam() {
  assert_eq!(std::env::var("TONIC_TEST_HEX").as_deref(), Ok("1"));
  let root = std::env::temp_dir().join(format!("tonicreq{}", std::process::id()));
  let project = root.join("project");
  copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/req"), &project);
  let mut server = Server(Command::new("python3")
    .args(["-u", "-c", r#"
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from urllib.parse import urlparse, parse_qs
class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *args): pass
    def reply(self, status, body, content_type="application/json", location=None):
        body = body.encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        if location: self.send_header("Location", location)
        self.end_headers()
        self.wfile.write(body)
    def do_GET(self):
        path = urlparse(self.path)
        if path.path == "/json":
            self.reply(200, json.dumps({"answer": parse_qs(path.query)["answer"][0], "header": self.headers["x-portable"]}))
        elif path.path == "/redirect": self.reply(302, "", location="/final")
        elif path.path == "/final": self.reply(200, "portable", "text/plain")
        else: self.reply(503, '{}')
    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        self.reply(201, body.decode())
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_port, flush=True)
server.serve_forever()
"#])
    .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap());
  let mut line = String::new();
  BufReader::new(server.0.stdout.take().unwrap()).read_line(&mut line).unwrap();
  let url = format!("http://127.0.0.1:{}", line.trim());
  let run = |executable: &str, args: &[&str], offline: bool| {
    let mut command = Command::new(executable);
    command.current_dir(&project).env("MIX_HOME", root.join("mixhome"))
      .env("HEX_HOME", root.join("hexhome")).env("MIX_ENV", "dev").args(args);
    if offline { command.env("HEX_OFFLINE", "1"); }
    process::execute(command, Duration::from_secs(600))
  };
  let success = |result: &std::process::Output| {
    assert!(result.status.success(), "stdout: {}\nstderr: {}",
      String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
  };
  success(&run("mix", &["local.hex", "--force"], false));
  success(&run("mix", &["deps.get"], false));
  success(&run("mix", &["compile"], true));
  let beam = run("mix", &["run", "--no-compile", "-e", "ReqProbe.main(System.argv())", "--", &url], true);
  success(&beam);
  assert_eq!(beam.stdout, b"{200, \"42\", \"yes\"}\n{201, true}\n{200, \"portable\"}\n503\n");
  let native = run(env!("CARGO_BIN_EXE_tonic"), &["run", ".", "--", &url], true);
  drop(server);
  success(&native);
  assert_eq!(native.stdout, beam.stdout);
  let original_lock = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/req/mix.lock")).unwrap();
  assert_eq!(fs::read(project.join("mix.lock")).unwrap(), original_lock);
  fs::remove_dir_all(root).unwrap();
}
