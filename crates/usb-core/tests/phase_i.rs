use std::{fs, path::Path};
use tempfile::TempDir;
use usb_core::{
    catalogue::{self, Catalogue},
    compatibility::{FAT32_MAX_FILE_BYTES, Filesystem},
    planner::{self, Options, SkipReason},
};

fn archive() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("archive.json"), r#"{"schema_version":1,"platforms":[{"id":"dos","name":"MS-DOS"},{"id":"c64","name":"Commodore 64"}],"collections":[{"id":"ancient","name":"Ancient Machines"}]}"#).unwrap();
    dir
}

fn game(root: &Path, id: &str, year: Option<u16>, size: u64) {
    let path = root.join(id);
    fs::create_dir_all(path.join("content")).unwrap();
    fs::write(path.join("game.json"), serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"id":id,"name":format!("Game {id}"),"platform":"dos","collection":"ancient",
        "release_year":year,"redistribution_approved":true,"approval_note":"Generated test fixture, not a game."
    })).unwrap()).unwrap();
    fs::File::create(path.join("content/data.bin"))
        .unwrap()
        .set_len(size)
        .unwrap();
}

fn edit(root: &Path, id: &str, key: &str, value: serde_json::Value) {
    let path = root.join(id).join("game.json");
    let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    data[key] = value;
    fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
}

fn options() -> Options {
    Options {
        margin_bytes: 0,
        margin_percent: 0,
        allocation_unit: 512,
        ..Options::default()
    }
}
fn plan(catalogue: &Catalogue, capacity: u64) -> planner::Plan {
    planner::plan(catalogue, capacity, Filesystem::Exfat, &options()).unwrap()
}
fn ids(plan: &planner::Plan) -> Vec<&str> {
    plan.selected.iter().map(|g| g.id.as_str()).collect()
}

#[test]
fn empty_and_tiny_capacity() {
    let dir = archive();
    let empty = catalogue::scan(dir.path()).unwrap();
    assert!(plan(&empty, 0).selected.is_empty());
    assert_eq!(plan(&empty, 100).remaining_safe_bytes, 100);
    game(dir.path(), "small", Some(1980), 1);
    let cat = catalogue::scan(dir.path()).unwrap();
    for capacity in [0, 1, 511, 2559] {
        assert!(plan(&cat, capacity).selected.is_empty());
    }
    assert_eq!(ids(&plan(&cat, 2560)), ["small"]); // file + metadata + root + two shared dirs
}

#[test]
fn age_size_id_order_is_independent_of_input_order() {
    let dir = archive();
    for (id, year, size) in [
        ("z", Some(1990), 1),
        ("b", Some(1980), 2),
        ("a", Some(1980), 2),
        ("tiny", Some(1980), 1),
        ("unknown", None, 0),
    ] {
        game(dir.path(), id, year, size);
    }
    let mut cat = catalogue::scan(dir.path()).unwrap();
    let first = plan(&cat, 1_000_000);
    assert_eq!(ids(&first), ["tiny", "a", "b", "z", "unknown"]);
    cat.games.reverse();
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&plan(&cat, 1_000_000)).unwrap()
    );
}

#[test]
fn manifest_and_config_priorities_override_age_then_sum_groups() {
    let dir = archive();
    game(dir.path(), "old", Some(1980), 1);
    game(dir.path(), "new", Some(2000), 1);
    edit(dir.path(), "new", "priority", 10.into());
    let cat = catalogue::scan(dir.path()).unwrap();
    assert_eq!(ids(&plan(&cat, 100_000)), ["new", "old"]);
    let mut opts = options();
    opts.priorities.games.insert("new".into(), -1);
    assert_eq!(
        ids(&planner::plan(&cat, 100_000, Filesystem::Exfat, &opts).unwrap()),
        ["old", "new"]
    );
    edit(dir.path(), "new", "platform", "c64".into());
    let cat = catalogue::scan(dir.path()).unwrap();
    opts.priorities.platforms.insert("c64".into(), 2);
    assert_eq!(
        ids(&planner::plan(&cat, 100_000, Filesystem::Exfat, &opts).unwrap()),
        ["new", "old"]
    );
    opts.priorities.games.insert("typo".into(), 1);
    assert!(planner::plan(&cat, 100_000, Filesystem::Exfat, &opts).is_err());
}

#[test]
fn oversized_games_are_skipped_and_small_games_fill_leftovers() {
    let dir = archive();
    game(dir.path(), "huge", Some(1970), 100_000);
    game(dir.path(), "first", Some(1980), 1024);
    game(dir.path(), "middle", Some(1990), 4096);
    game(dir.path(), "last", Some(2000), 512);
    let result = plan(&catalogue::scan(dir.path()).unwrap(), 4608);
    assert_eq!(ids(&result), ["first", "last"]);
    assert_eq!(result.remaining_safe_bytes, 0);
    assert_eq!(result.skipped.len(), 2);
    assert!(matches!(
        result.skipped[0].reason,
        SkipReason::InsufficientCapacity { .. }
    ));
    assert_eq!(result.payload_bytes, 1536);
}

#[test]
fn approval_is_explicit_and_needs_evidence() {
    let dir = archive();
    game(dir.path(), "unapproved", Some(1980), 1);
    edit(
        dir.path(),
        "unapproved",
        "redistribution_approved",
        false.into(),
    );
    let result = plan(&catalogue::scan(dir.path()).unwrap(), 1_000_000);
    assert!(matches!(result.skipped[0].reason, SkipReason::Unapproved));
    let path = dir.path().join("unapproved/game.json");
    let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    data.as_object_mut()
        .unwrap()
        .remove("redistribution_approved");
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(
        plan(&catalogue::scan(dir.path()).unwrap(), 1_000_000)
            .selected
            .is_empty()
    );
    edit(
        dir.path(),
        "unapproved",
        "redistribution_approved",
        true.into(),
    );
    edit(
        dir.path(),
        "unapproved",
        "approval_note",
        serde_json::Value::Null,
    );
    assert!(catalogue::scan(dir.path()).is_err());
}

#[test]
fn fat32_checks_individual_files_not_game_total() {
    let dir = archive();
    game(dir.path(), "large", Some(1990), FAT32_MAX_FILE_BYTES + 1);
    let cat = catalogue::scan(dir.path()).unwrap();
    assert!(
        planner::plan(&cat, 20 << 30, Filesystem::Fat32, &options())
            .unwrap()
            .selected
            .is_empty()
    );
    assert_eq!(ids(&plan(&cat, 20 << 30)), ["large"]);
    fs::File::create(dir.path().join("large/content/data.bin"))
        .unwrap()
        .set_len(FAT32_MAX_FILE_BYTES)
        .unwrap();
    fs::File::create(dir.path().join("large/content/second.bin"))
        .unwrap()
        .set_len(FAT32_MAX_FILE_BYTES)
        .unwrap();
    let cat = catalogue::scan(dir.path()).unwrap();
    assert_eq!(
        planner::plan(&cat, 20 << 30, Filesystem::Fat32, &options())
            .unwrap()
            .selected
            .len(),
        1
    );
}

#[test]
fn duplicate_ids_and_invalid_metadata_fail_closed() {
    let dir = archive();
    game(dir.path(), "a", Some(1980), 1);
    game(dir.path(), "b", Some(1980), 1);
    edit(dir.path(), "b", "id", "a".into());
    assert!(
        catalogue::scan(dir.path())
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    edit(dir.path(), "b", "id", "../escape".into());
    assert!(catalogue::scan(dir.path()).is_err());
    edit(dir.path(), "b", "id", "b".into());
    edit(dir.path(), "b", "platform", "missing".into());
    assert!(catalogue::scan(dir.path()).is_err());
    edit(dir.path(), "b", "platform", "dos".into());
    edit(dir.path(), "b", "schema_version", 99.into());
    assert!(catalogue::scan(dir.path()).is_err());
}

#[test]
fn filename_path_and_case_incompatibilities() {
    for name in [
        "CON.txt",
        "bad?.bin",
        "trailing.",
        "trailing ",
        "é.bin",
        "bad\\name",
    ] {
        let dir = archive();
        game(dir.path(), "a", Some(1980), 1);
        fs::rename(
            dir.path().join("a/content/data.bin"),
            dir.path().join("a/content").join(name),
        )
        .unwrap();
        let cat = catalogue::scan(dir.path()).unwrap();
        assert!(plan(&cat, 1_000_000).selected.is_empty(), "{name}");
    }
    let dir = archive();
    game(dir.path(), "a", Some(1980), 1);
    fs::write(dir.path().join("a/content/DATA.BIN"), b"x").unwrap();
    let cat = catalogue::scan(dir.path()).unwrap();
    assert!(plan(&cat, 1_000_000).selected.is_empty());
    assert_eq!(
        planner::plan(&cat, 1_000_000, Filesystem::Ext4, &options())
            .unwrap()
            .selected
            .len(),
        1
    );
    fs::remove_file(dir.path().join("a/content/DATA.BIN")).unwrap();
    fs::create_dir_all(dir.path().join("a/content").join("x".repeat(230))).unwrap();
    assert!(
        plan(&catalogue::scan(dir.path()).unwrap(), 1_000_000)
            .selected
            .is_empty()
    );
}

#[test]
fn explicit_filesystem_restrictions_apply() {
    let dir = archive();
    game(dir.path(), "a", None, 1);
    edit(dir.path(), "a", "filesystems", serde_json::json!(["ext4"]));
    let cat = catalogue::scan(dir.path()).unwrap();
    assert!(plan(&cat, 1_000_000).selected.is_empty());
    assert_eq!(
        planner::plan(&cat, 1_000_000, Filesystem::Ext4, &options())
            .unwrap()
            .selected
            .len(),
        1
    );
}

#[test]
fn margins_allocation_and_overflow_boundaries() {
    let dir = archive();
    game(dir.path(), "a", None, 1);
    let cat = catalogue::scan(dir.path()).unwrap();
    let mut opts = options();
    opts.margin_bytes = 1000;
    opts.margin_percent = 10;
    let result = planner::plan(&cat, 3000, Filesystem::Exfat, &opts).unwrap();
    assert_eq!(result.safe_capacity, 2000);
    assert!(result.selected.is_empty());
    opts.margin_bytes = 0;
    let result = planner::plan(&cat, 3001, Filesystem::Exfat, &opts).unwrap();
    assert_eq!(result.reserved_bytes, 301);
    assert_eq!(result.planned_bytes, 2560);
    opts.margin_percent = 100;
    assert_eq!(
        planner::plan(&cat, u64::MAX, Filesystem::Exfat, &opts)
            .unwrap()
            .safe_capacity,
        0
    );
    opts.margin_percent = 101;
    assert!(planner::plan(&cat, 10, Filesystem::Exfat, &opts).is_err());
    opts.margin_percent = 0;
    opts.allocation_unit = 513;
    assert!(planner::plan(&cat, 10, Filesystem::Exfat, &opts).is_err());
    let mut cat = cat;
    cat.games[0].files[0].bytes = u64::MAX;
    cat.games[0].bytes = u64::MAX;
    assert!(planner::plan(&cat, u64::MAX, Filesystem::Ext4, &options()).is_err());
}

#[test]
fn allocation_accounts_for_empty_files_and_directories() {
    let dir = archive();
    game(dir.path(), "a", None, 0);
    fs::create_dir(dir.path().join("a/content/empty")).unwrap();
    let result = plan(&catalogue::scan(dir.path()).unwrap(), 2560);
    assert_eq!(result.planned_bytes, 2560);
    assert_eq!(result.payload_bytes, 0);
}

#[test]
fn catalogue_sizes_respect_capacity_and_report_every_game() {
    for count in [1, 10, 100, 1000] {
        let dir = archive();
        for i in 0..count {
            game(
                dir.path(),
                &format!("g{i:04}"),
                Some(1980 + (i % 30) as u16),
                i * 13,
            );
        }
        let cat = catalogue::scan(dir.path()).unwrap();
        for capacity in [0, 10_000, 1_000_000, u64::MAX] {
            let result = plan(&cat, capacity);
            assert!(result.planned_bytes <= result.safe_capacity);
            assert_eq!(result.selected.len() + result.skipped.len(), count as usize);
            assert_eq!(
                result.planned_bytes + result.remaining_safe_bytes,
                result.safe_capacity
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn symlinks_special_files_and_non_utf8_payloads_are_rejected() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let dir = archive();
    game(dir.path(), "a", None, 1);
    let bad = dir.path().join("a/content/bad");
    symlink("/does-not-exist", &bad).unwrap();
    assert!(catalogue::scan(dir.path()).is_err());
    fs::remove_file(&bad).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&bad)
            .status()
            .unwrap()
            .success()
    );
    assert!(catalogue::scan(dir.path()).is_err());
    fs::remove_file(&bad).unwrap();
    let name = std::ffi::OsString::from_vec(vec![0xff]);
    fs::write(dir.path().join("a/content").join(name), b"x").unwrap();
    assert!(catalogue::scan(dir.path()).is_err());
}

#[test]
fn unknown_year_is_after_every_known_year_and_unmanifested_files_are_not_games() {
    let dir = archive();
    fs::create_dir(dir.path().join("unmanifested")).unwrap();
    fs::write(dir.path().join("unmanifested/retro.bin"), b"not approved").unwrap();
    assert!(catalogue::scan(dir.path()).unwrap().games.is_empty());
    game(dir.path(), "known", Some(u16::MAX), 2);
    game(dir.path(), "unknown", None, 1);
    assert_eq!(
        ids(&plan(&catalogue::scan(dir.path()).unwrap(), 100_000)),
        ["known", "unknown"]
    );
}

#[test]
fn malicious_relative_paths_cannot_escape_the_distribution_tree() {
    let dir = archive();
    game(dir.path(), "a", None, 1);
    let mut cat = catalogue::scan(dir.path()).unwrap();
    for path in [
        "../escape",
        "/absolute",
        "a/../../escape",
        "a//file",
        "./file",
        "a\\file",
    ] {
        cat.games[0].files[0].path = path.into();
        for filesystem in [Filesystem::Fat32, Filesystem::Exfat, Filesystem::Ext4] {
            let result = planner::plan(&cat, 100_000, filesystem, &options()).unwrap();
            assert!(
                matches!(result.skipped[0].reason, SkipReason::Incompatible(_)),
                "{path}"
            );
        }
    }
}
