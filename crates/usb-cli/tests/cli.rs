use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_usb"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn documented_commands_and_json_work_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("archive.json"),
        r#"{"schema_version":1,"platforms":[],"collections":[]}"#,
    )
    .unwrap();
    let root = dir.path().to_str().unwrap();
    let scan = run(&["catalogue", "scan", root, "--json"]);
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    let cat: serde_json::Value = serde_json::from_slice(&scan.stdout).unwrap();
    assert_eq!(cat["games"], serde_json::json!([]));
    assert!(
        run(&["catalogue", "list", "--archive", root])
            .status
            .success()
    );
    for fs in ["exfat", "fat32", "ext4"] {
        let result = run(&[
            "plan",
            "--archive",
            root,
            "--capacity",
            "16G",
            "--filesystem",
            fs,
            "--json",
        ]);
        assert!(result.status.success());
        let plan: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(plan["available_bytes"], 16_u64 << 30);
        assert_eq!(plan["planned_bytes"], 0);
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn config_overrides_and_failures_are_actionable() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("archive.json"),
        r#"{"schema_version":1,"platforms":[],"collections":[]}"#,
    )
    .unwrap();
    let config = dir.path().join("config.json");
    fs::write(&config, serde_json::to_vec(&serde_json::json!({"archive":dir.path(),"planner":{"margin_bytes":123,"margin_percent":0}})).unwrap()).unwrap();
    let result = run(&[
        "--config",
        config.to_str().unwrap(),
        "plan",
        "--capacity",
        "1000",
        "--filesystem",
        "fat32",
        "--margin-bytes",
        "200",
        "--json",
    ]);
    assert!(result.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(plan["reserved_bytes"], 200);
    assert!(
        !run(&["plan", "--capacity", "-1", "--filesystem", "fat32"])
            .status
            .success()
    );
    assert!(
        !run(&["plan", "--capacity", "1G", "--filesystem", "ntfs"])
            .status
            .success()
    );
    let missing = run(&["catalogue", "scan", "/not-an-existing-usb-archive"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("/not-an-existing-usb-archive"));
}

#[test]
fn directory_copy_reports_verified_games_and_refuses_repeated_writes() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    fs::write(source.path().join("archive.json"), r#"{"schema_version":1,"platforms":[{"id":"p","name":"P"}],"collections":[{"id":"c","name":"C"}]}"#).unwrap();
    fs::create_dir_all(source.path().join("a/content")).unwrap();
    fs::write(
        source.path().join("a/content/demo.txt"),
        b"generated fixture",
    )
    .unwrap();
    fs::write(source.path().join("a/game.json"), r#"{"schema_version":1,"id":"a","name":"A","platform":"p","collection":"c","redistribution_approved":true,"approval_note":"Generated fixture"}"#).unwrap();
    fs::write(destination.path().join("personal.txt"), b"untouched").unwrap();
    let args = [
        "--archive",
        source.path().to_str().unwrap(),
        "--json",
        "copy-to-directory",
        "--destination",
        destination.path().to_str().unwrap(),
        "--filesystem",
        "ext4",
    ];
    let copied = run(&args);
    assert!(
        copied.status.success(),
        "{}",
        String::from_utf8_lossy(&copied.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&copied.stdout).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["completed"][0]["id"], "a");
    assert_eq!(
        report["completed"][0]["files"][0]["sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        fs::read(destination.path().join("USB/games/a/demo.txt")).unwrap(),
        b"generated fixture"
    );
    for line in String::from_utf8(copied.stderr).unwrap().lines() {
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(event["event"].is_string());
    }
    let repeated = run(&args);
    assert!(!repeated.status.success());
    assert!(String::from_utf8_lossy(&repeated.stderr).contains("already contains USB"));
    let inspected = run(&[
        "inspect-transfer",
        "--destination",
        destination.path().to_str().unwrap(),
        "--json",
    ]);
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspection: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(inspection["terminal_status"], "completed");
    assert_eq!(inspection["recorded_completed"], serde_json::json!(["a"]));
    assert_eq!(inspection["publication_uncertain"], false);
    assert!(inspection["active_game"].is_null());
    assert_eq!(
        fs::read(destination.path().join("personal.txt")).unwrap(),
        b"untouched"
    );
}

#[test]
fn batch_reports_jobs_individually_and_preserves_colliding_destination() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/archive")
        .canonicalize()
        .unwrap();
    let good = tempfile::tempdir().unwrap();
    let bad = tempfile::tempdir().unwrap();
    fs::write(bad.path().join("USB"), b"personal").unwrap();
    let copied = run(&[
        "--archive",
        source.to_str().unwrap(),
        "--json",
        "copy-batch",
        "--destination",
        bad.path().to_str().unwrap(),
        "--destination",
        good.path().to_str().unwrap(),
        "--workers",
        "2",
        "--filesystem",
        "ext4",
    ]);
    assert!(!copied.status.success());
    let report: serde_json::Value = serde_json::from_slice(&copied.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["jobs"][0]["id"], "port-1");
    assert_eq!(report["jobs"][0]["status"], "failed");
    assert_eq!(report["jobs"][1]["status"], "completed");
    assert_eq!(
        report["jobs"][1]["report"]["completed"][0]["id"],
        "bureau-demo"
    );
    assert_eq!(fs::read(bad.path().join("USB")).unwrap(), b"personal");
    for line in String::from_utf8(copied.stderr)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with('{'))
    {
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(event["event"].is_string());
        assert!(event["job_id"].is_string());
    }
}

#[test]
fn batch_rejects_duplicate_destinations_and_invalid_worker_limits_before_writes() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/archive")
        .canonicalize()
        .unwrap();
    let dest = tempfile::tempdir().unwrap();
    for workers in ["0", "11", "2"] {
        let result = run(&[
            "--archive",
            source.to_str().unwrap(),
            "copy-batch",
            "--destination",
            dest.path().to_str().unwrap(),
            "--destination",
            dest.path().to_str().unwrap(),
            "--workers",
            workers,
            "--filesystem",
            "ext4",
        ]);
        assert!(!result.status.success());
        assert!(!dest.path().join("USB").exists());
    }
}

#[test]
fn device_snapshot_is_read_only_and_never_becomes_live_metadata() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/devices.json")
        .canonicalize()
        .unwrap();
    let before = fs::read(&fixture).unwrap();
    let result = run(&["devices", "--snapshot", fixture.to_str().unwrap(), "--json"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let inventory: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(inventory["live_snapshot"], false);
    assert_eq!(inventory["devices"][1]["candidate_for_review"], true);
    assert_eq!(fs::read(&fixture).unwrap(), before);
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("invalid.json");
    fs::write(&invalid, b"{\"blockdevices\":[],\"extra\":true}").unwrap();
    assert!(
        !run(&["devices", "--snapshot", invalid.to_str().unwrap()])
            .status
            .success()
    );
    let link = dir.path().join("link.json");
    std::os::unix::fs::symlink(fixture, &link).unwrap();
    assert!(
        !run(&["devices", "--snapshot", link.to_str().unwrap()])
            .status
            .success()
    );
}
