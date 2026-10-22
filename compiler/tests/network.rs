#[path = "support/process.rs"]
mod process;

use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn tls_callbacks_cipher_policy_and_udp_socket_options_match_beam() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/network.exs");
    let mut reference = Command::new("elixir");
    reference.current_dir(root).arg(&source);
    let reference = process::execute(reference, Duration::from_secs(40));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );

    let dir = std::env::temp_dir().join(format!("tonicnetwork{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let binary = dir.join("network");
    let mut build = Command::new(env!("CARGO_BIN_EXE_tonic"));
    build
        .current_dir(root)
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&binary);
    let build = process::execute(build, Duration::from_secs(240));
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    for stress in ["0", "1"] {
        let mut native = Command::new(&binary);
        native.current_dir(root).env("TONIC_GC_STRESS", stress);
        let native = process::execute(native, Duration::from_secs(40));
        assert!(
            native.status.success(),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&reference.stdout)
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn tls_callbacks_stop_on_timeout_owner_exit_and_socket_close() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let source = root.join("tests/fixtures/networkcancel.exs");
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native
        .current_dir(root)
        .env("TONIC_GC_STRESS", "1")
        .arg("run")
        .arg(&source);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "callback timeout cleaned\ncallback owner exit cleaned\ncallback socket close cleaned\n"
    );
}
