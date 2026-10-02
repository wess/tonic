#[path = "support/process.rs"]
mod process;

use std::process::Command;
use std::time::Duration;

#[test]
fn supervised_messages_restarts_and_moving_gc_survive_sustained_load() {
    let seconds = std::env::var("TONIC_SOAK_SECONDS").unwrap_or_else(|_| "5".into());
    let duration: u64 = seconds.parse().unwrap();
    assert!((1..=120).contains(&duration));
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/soak.exs");
    let mut reference = Command::new("elixir");
    reference.arg(&source).arg(&seconds);
    let reference = process::execute(reference, Duration::from_secs(duration + 90));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert!(String::from_utf8_lossy(&reference.stdout).contains("soak:ok "));
    eprintln!(
        "Elixir: {}",
        String::from_utf8_lossy(&reference.stdout)
            .lines()
            .find(|line| line.starts_with("soak:ok "))
            .unwrap()
    );
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native
        .env("TONIC_GC_STRESS", "1")
        .args(["run", source.to_str().unwrap(), "--", &seconds]);
    let native = process::execute(native, Duration::from_secs(duration + 180));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert!(String::from_utf8_lossy(&native.stdout).contains("soak:ok "));
    eprintln!(
        "Tonic GC stress: {}",
        String::from_utf8_lossy(&native.stdout)
            .lines()
            .find(|line| line.starts_with("soak:ok "))
            .unwrap()
    );
}
