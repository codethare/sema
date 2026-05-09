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
    assert!(stdout.contains("0.0.3"));
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
