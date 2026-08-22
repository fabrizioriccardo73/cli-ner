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
fn test_cli_clean_dev_dry_run() {
    let output = Command::new("cargo")
        .args(["run", "--", "clean", "--category", "dev"])
        .output()
        .expect("Failed to execute cargo run -- clean --category dev");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN MODE"));
    assert!(stdout.contains("Cleaning Targets Summary"));
}

#[test]
fn test_cli_clean_gradle_dry_run() {
    let output = Command::new("cargo")
        .args(["run", "--", "clean", "--category", "gradle"])
        .output()
        .expect("Failed to execute cargo run -- clean --category gradle");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN MODE"));
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

#[test]
fn test_cli_projects_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "projects", "--help"])
        .output()
        .expect("Failed to execute cargo run -- projects --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dormant software project"));
    assert!(stdout.contains("--days"));
    assert!(stdout.contains("--type"));
    assert!(stdout.contains("--interactive"));
    assert!(stdout.contains("--execute"));
}

#[test]
fn test_cli_projects_scan_json() {
    let output = Command::new("cargo")
        .args(["run", "--", "projects", "--path", ".", "--all", "--format", "json"])
        .output()
        .expect("Failed to execute cargo run -- projects --format json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("root_path") || stdout.contains("[]"));
}

#[test]
fn test_cli_projects_sweep_alias() {
    let output = Command::new("cargo")
        .args(["run", "--", "sweep", "--help"])
        .output()
        .expect("Failed to execute cargo run -- sweep --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dormant software project"));
}

#[test]
fn test_cli_tui_alias_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "tui", "--help"])
        .output()
        .expect("Failed to execute cargo run -- tui --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Dashboard") || stdout.contains("Terminal UI"));
}

#[test]
fn test_cli_projects_table_formatting() {
    let output = Command::new("cargo")
        .args(["run", "--", "projects", "--path", ".", "--all"])
        .output()
        .expect("Failed to execute cargo run -- projects --path . --all");

    assert!(output.status.success());
}

#[test]
fn test_cli_bloat() {
    let output = Command::new("cargo")
        .args(["run", "--", "bloat", "--min-size", "10MB"])
        .output()
        .expect("Failed to execute cargo run -- bloat");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Phantom Disk Space Bloat Analysis"));
    assert!(stdout.contains("Disk Status"));
}

#[test]
fn test_cli_bloat_json() {
    let output = Command::new("cargo")
        .args(["run", "--", "bloat", "--format", "json"])
        .output()
        .expect("Failed to execute cargo run -- bloat --format json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("total_bloat_bytes"));
    assert!(stdout.contains("sources"));
}

#[test]
fn test_cli_bloat_detailed() {
    let output = Command::new("cargo")
        .args(["run", "--", "bloat", "--detailed", "--min-size", "10MB"])
        .output()
        .expect("Failed to execute cargo run -- bloat --detailed");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Phantom Disk Space Bloat Analysis"));
}

#[test]
fn test_cli_phantom_alias() {
    let output = Command::new("cargo")
        .args(["run", "--", "phantom", "--help"])
        .output()
        .expect("Failed to execute cargo run -- phantom --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("phantom disk space"));
}

#[test]
fn test_cli_snapshot_create_and_list() {
    let output = Command::new("cargo")
        .args(["run", "--", "snapshot", "create", "--name", "test-ci-snap", "--path", "."])
        .output()
        .expect("Failed to execute cargo run -- snapshot create");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Snapshot Captured Successfully"));

    let list_output = Command::new("cargo")
        .args(["run", "--", "snapshot", "list"])
        .output()
        .expect("Failed to execute cargo run -- snapshot list");

    assert!(list_output.status.success());
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(list_stdout.contains("test-ci-snap") || list_stdout.contains("Saved Disk Snapshots"));
}

#[test]
fn test_cli_diff() {
    let output = Command::new("cargo")
        .args(["run", "--", "diff", "--top", "5", "--depth", "1"])
        .output()
        .expect("Failed to execute cargo run -- diff");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Disk Differential Comparison") || stdout.contains("Baseline snapshot"));
}

