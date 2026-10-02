use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn host_preserves_frames_and_reports_source_loading_failures() {
    let dir = std::env::temp_dir().join(format!("tonichost{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cache = dir.join("cache");
    let source = dir.join("output.exs");
    std::fs::write(
        &source,
        r#"
defmodule Outputs do
  defmacro answer do
    IO.puts("compile time output")
    length([1, 2, 3])
  end
end
require Outputs
IO.inspect(Outputs.answer())
"#,
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_tonic"))
        .env("TONIC_CACHE", &cache)
        .arg("run")
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "3\n");
    assert!(String::from_utf8_lossy(&result.stderr).contains("compile time output"));

    let host = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("macrohost-")
                && p.extension().is_none()
        })
        .unwrap();
    let broken = dir.join("broken.ex");
    std::fs::write(
        &broken,
        r#"
defmodule Broken do
  @value raise "source load exploded"
  defmacro broken do
    length([1])
  end
end
"#,
    )
    .unwrap();
    let missing = dir.join("missing.ex");
    let invalid = dir.join("invalid.ex");
    std::fs::write(&invalid, "defmodule Invalid do\n").unwrap();
    let callbacks = dir.join("callbacks.ex");
    std::fs::write(
        &callbacks,
        r#"
defmodule Artifact do
  def __after_compile__(_env, bytes) do
    unless is_binary(bytes) and byte_size(bytes) > 1000000, do: raise("missing native artifact")
    :ok
  end
end
defmodule Subject do
  @after_compile Artifact
end
"#,
    )
    .unwrap();
    let requests = [
        ("LOAD", missing.to_string_lossy().into_owned()),
        ("LOAD", invalid.to_string_lossy().into_owned()),
        ("LOAD", broken.to_string_lossy().into_owned()),
        ("EXPAND", "{Broken, :broken, [], []}".into()),
        ("LOAD", callbacks.to_string_lossy().into_owned()),
        ("AFTER", format!("Subject\n{}", "\n\"\\\n".repeat(250001))),
    ];
    let mut child = Command::new(host)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for (command, payload) in requests {
        write!(stdin, "{} {}\n{}", command, payload.len(), payload).unwrap();
    }
    drop(stdin);
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut bytes = result.stdout.as_slice();
    let mut replies = Vec::new();
    while !bytes.is_empty() {
        let newline = bytes.iter().position(|b| *b == b'\n').unwrap();
        let header = std::str::from_utf8(&bytes[..newline]).unwrap();
        let (tag, size) = header.split_once(' ').unwrap();
        let size = size.parse::<usize>().unwrap();
        bytes = &bytes[newline + 1..];
        replies.push((
            tag.to_string(),
            String::from_utf8(bytes[..size].to_vec()).unwrap(),
        ));
        bytes = &bytes[size..];
    }
    assert_eq!(replies.len(), 6);
    assert_eq!(replies[0].0, "ERR");
    assert!(replies[0].1.contains("missing.ex"), "{:?}", replies);
    assert_eq!(replies[1].0, "ERR");
    assert!(replies[1].1.contains("invalid.ex"), "{:?}", replies);
    assert_eq!(replies[2].0, "OK");
    assert_eq!(replies[3].0, "ERR");
    assert!(
        replies[3].1.contains("source load exploded"),
        "{:?}",
        replies
    );
    assert!(replies[3].1.contains("broken.ex"), "{:?}", replies);
    assert_eq!(replies[4].0, "OK", "{:?}", replies);
    assert_eq!(replies[5].0, "OK", "{:?}", replies);
    std::fs::remove_dir_all(dir).unwrap();
}
