use std::process::Command;

#[test]
fn dry_run_output_contains_all_metrics() {
    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .arg("--dry-run")
        .output()
        .expect("failed to run sema --dry-run");

    assert!(output.status.success(), "sema --dry-run exited with code {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("sema"), "output should contain program name");
    assert!(stdout.contains("config:"), "output should show config path");
    assert!(stdout.contains("System state:"), "output should show system state header");
    assert!(stdout.contains("CPU"), "output should show CPU metric");
    assert!(stdout.contains("Memory"), "output should show Memory metric");
    assert!(stdout.contains("Swap"), "output should show Swap metric");
    assert!(stdout.contains("Battery"), "output should show Battery metric");
    assert!(stdout.contains("Time"), "output should show Time metric");
    assert!(stdout.contains("Temp"), "output should show Temp metric");
    assert!(stdout.contains("Network"), "output should show Network metric");
}

#[test]
fn version_flag_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .arg("--version")
        .output()
        .expect("failed to run sema --version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sema"));
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "version string not found, expected {}",
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn help_flag_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .arg("--help")
        .output()
        .expect("failed to run sema --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--version"));
    assert!(stdout.contains("--init"));
}

#[test]
fn init_creates_config_file() {
    let tmp = std::env::temp_dir().join(format!("sema_test_init_{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);

    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .args(["--config", tmp.to_str().unwrap(), "--init"])
        .output()
        .expect("failed to run sema --init");
    assert!(output.status.success(), "sema --init failed: {}", String::from_utf8_lossy(&output.stderr));
    assert!(tmp.exists(), "config file should exist after --init");

    let content = std::fs::read_to_string(&tmp).unwrap_or_default();
    assert!(content.contains("threshold"), "config should contain threshold settings");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn init_refuses_to_overwrite_without_force() {
    let tmp = std::env::temp_dir().join(format!("sema_test_overwrite_{}", std::process::id()));
    std::fs::write(&tmp, b"existing content").expect("write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .args(["--config", tmp.to_str().unwrap(), "--init"])
        .output()
        .expect("failed to run sema --init");
    assert!(!output.status.success(), "--init should refuse to overwrite");
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));

    // file should still have original content
    let content = std::fs::read_to_string(&tmp).unwrap_or_default();
    assert_eq!(content, "existing content");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn init_force_overwrites_existing() {
    let tmp = std::env::temp_dir().join(format!("sema_test_force_{}", std::process::id()));
    std::fs::write(&tmp, b"old content").expect("write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .args(["--config", tmp.to_str().unwrap(), "--init", "--force"])
        .output()
        .expect("failed to run sema --init --force");
    assert!(output.status.success(), "--init --force should succeed");

    let content = std::fs::read_to_string(&tmp).unwrap_or_default();
    assert!(content.contains("threshold"), "force overwritten config should contain threshold");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn custom_config_with_dry_run() {
    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .args(["-c", "/nonexistent/sema/config.toml", "--dry-run"])
        .output()
        .expect("failed to run sema with custom config");
    assert!(output.status.success(), "dry-run with non-existent config should use defaults");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("config:"), "should print config path");
    assert!(stdout.contains("System state:"), "should show system state");
}

#[test]
fn config_without_value_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .args(["-c"])
        .output()
        .expect("failed to run sema");
    assert_eq!(output.status.code(), Some(2), "-c without value should exit with code 2");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--config"), "error should mention --config");
}

#[test]
fn test_flag_errors_without_display() {
    let output = Command::new(env!("CARGO_BIN_EXE_sema"))
        .arg("--test")
        .output()
        .expect("failed to run sema --test");
    // In a non-graphical environment, --test should fail
    assert_eq!(output.status.code(), Some(1), "--test should fail without display");
}
