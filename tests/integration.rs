use std::process::Command;

#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to execute cargo run -- --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CLI-NER"));
    assert!(stdout.contains("scan"));
    assert!(stdout.contains("clean"));
    assert!(stdout.contains("report"));
    assert!(stdout.contains("doctor"));
}

#[test]
fn test_cli_doctor() {
    let output = Command::new("cargo")
        .args(["run", "--", "doctor"])
        .output()
        .expect("Failed to execute cargo run -- doctor");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Mounted Disks & Storage"));
    assert!(stdout.contains("External Developer Tools Availability"));
    assert!(stdout.contains("Safety Guarantees & Protection"));
}

#[test]
fn test_cli_clean_dry_run() {
    let output = Command::new("cargo")
        .args(["run", "--", "clean", "--category", "user-logs"])
        .output()
        .expect("Failed to execute cargo run -- clean");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN MODE"));
    assert!(stdout.contains("Cleaning Targets Summary"));
}

#[test]
fn test_cli_scan() {
    let output = Command::new("cargo")
        .args(["run", "--", "scan", "--path", ".", "--top", "5"])
        .output()
        .expect("Failed to execute cargo run -- scan");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Scanned Target"));
    assert!(stdout.contains("Item Name"));
}

#[test]
fn test_cli_scan_json() {
    let output = Command::new("cargo")
        .args(["run", "--", "scan", "--path", ".", "--format", "json"])
        .output()
        .expect("Failed to execute cargo run -- scan json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"target_path\""));
    assert!(stdout.contains("\"entries\""));
}

#[test]
fn test_cli_docker_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "docker", "--help"])
        .output()
        .expect("Failed to execute cargo run -- docker --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Safe interactive Docker manager"));
    assert!(stdout.contains("containers"));
    assert!(stdout.contains("images"));
    assert!(stdout.contains("volumes"));
}

#[test]
fn test_cli_docker_status() {
    let output = Command::new("cargo")
        .args(["run", "--", "docker", "status"])
        .output()
        .expect("Failed to execute cargo run -- docker status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Docker Component") || stdout.contains("Docker is not available"));
}

#[test]
fn test_cli_docker_containers_list() {
    let output = Command::new("cargo")
        .args(["run", "--", "docker", "containers", "--list"])
        .output()
        .expect("Failed to execute cargo run -- docker containers --list");

    assert!(output.status.success());
}

#[test]
fn test_cli_docker_images_list() {
    let output = Command::new("cargo")
        .args(["run", "--", "docker", "images", "--list"])
        .output()
        .expect("Failed to execute cargo run -- docker images --list");

    assert!(output.status.success());
}

#[test]
fn test_cli_docker_volumes() {
    let output = Command::new("cargo")
        .args(["run", "--", "docker", "volumes"])
        .output()
        .expect("Failed to execute cargo run -- docker volumes");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DOCKER VOLUMES & PERSISTENT DATA SAFETY AUDIT")
            || stdout.contains("Docker is not available")
    );
}
