use super::*;
use std::{
    fs as stdfs,
    io::{self as stdio, Cursor, Read, Write},
    os::unix::fs::{PermissionsExt, symlink},
    sync::atomic::AtomicBool,
};
use tempfile::TempDir;

#[path = "durability_tests.rs"]
mod durability;

#[path = "scheduler_tests.rs"]
mod scheduling;

fn archive() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    stdfs::write(root.path().join("archive.json"), r#"{"schema_version":1,"platforms":[{"id":"p","name":"Platform"}],"collections":[{"id":"c","name":"Collection"}]}"#).unwrap();
    root
}
fn game(root: &Path, id: &str, bytes: usize) {
    let base = root.join(id);
    stdfs::create_dir_all(base.join("content/extras/empty")).unwrap();
    stdfs::write(base.join("content/data.bin"), vec![0x5a; bytes]).unwrap();
    stdfs::write(
        base.join("content/extras/LICENCE.txt"),
        b"Generated test fixture; freely redistributable.",
    )
    .unwrap();
    stdfs::write(base.join("game.json"), serde_json::to_vec(&serde_json::json!({"schema_version":1,"id":id,"name":id,"platform":"p","collection":"c","release_year":1980,"redistribution_approved":true,"approval_note":"Original generated test data"})).unwrap()).unwrap();
}
fn options() -> TransferOptions {
    TransferOptions {
        filesystem: Filesystem::Ext4,
        planner: Options {
            margin_bytes: 4096,
            margin_percent: 0,
            allocation_unit: 4096,
            ..Options::default()
        },
        capacity_limit: Some(16 * 1024 * 1024),
    }
}
fn transfer(archive: &Path, destination: &Path) -> Result<Report> {
    copy_to_directory(
        archive,
        destination,
        &options(),
        &AtomicBool::new(false),
        |_| {},
    )
}

#[test]
fn copies_only_complete_verified_games_and_preserves_personal_data() {
    let source = archive();
    game(source.path(), "a", 300_000);
    game(source.path(), "b", 4);
    let dest = tempfile::tempdir().unwrap();
    stdfs::write(dest.path().join("personal.txt"), b"untouched").unwrap();
    let executable = source.path().join("a/content/data.bin");
    stdfs::set_permissions(&executable, stdfs::Permissions::from_mode(0o4755)).unwrap();
    let mut finished = 0;
    let report = copy_to_directory(
        source.path(),
        dest.path(),
        &options(),
        &AtomicBool::new(false),
        |event| {
            if let Event::GameCompleted { id } = event {
                finished += 1;
                assert!(
                    dest.path()
                        .join("USB/games")
                        .join(id)
                        .join("extras/LICENCE.txt")
                        .is_file()
                );
            }
        },
    )
    .unwrap();
    assert_eq!(report.status, Status::Completed);
    assert_eq!(report.completed.len(), 2);
    assert_eq!(finished, 2);
    assert_eq!(
        stdfs::read(dest.path().join("personal.txt")).unwrap(),
        b"untouched"
    );
    for id in ["a", "b"] {
        let copied = dest.path().join("USB/games").join(id);
        assert_eq!(
            stdfs::read(copied.join("data.bin")).unwrap(),
            stdfs::read(source.path().join(id).join("content/data.bin")).unwrap()
        );
        assert!(copied.join("extras/empty").is_dir());
        assert!(!dest.path().join("USB/.incoming").join(id).exists());
    }
    assert_eq!(
        stdfs::metadata(dest.path().join("USB/games/a/data.bin"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o755
    );
    assert!(
        report
            .completed
            .iter()
            .all(|g| g.files.iter().all(|f| f.sha256.len() == 64))
    );
}

#[test]
fn empty_or_tiny_budget_does_not_create_distribution_directories() {
    let source = archive();
    let dest = tempfile::tempdir().unwrap();
    assert_eq!(
        transfer(source.path(), dest.path()).unwrap().status,
        Status::Completed
    );
    assert!(!dest.path().join("USB").exists());
    game(source.path(), "a", 10);
    let mut opts = options();
    opts.capacity_limit = Some(1);
    let report = copy_to_directory(
        source.path(),
        dest.path(),
        &opts,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(report.completed.is_empty());
    assert!(report.plan.selected.is_empty());
    assert!(!dest.path().join("USB").exists());
}

#[test]
fn unapproved_and_incompatible_games_never_get_copied() {
    let source = archive();
    game(source.path(), "a", 1);
    let manifest = source.path().join("a/game.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&stdfs::read(&manifest).unwrap()).unwrap();
    data["redistribution_approved"] = false.into();
    stdfs::write(&manifest, serde_json::to_vec(&data).unwrap()).unwrap();
    let dest = tempfile::tempdir().unwrap();
    assert!(
        transfer(source.path(), dest.path())
            .unwrap()
            .completed
            .is_empty()
    );
    data["redistribution_approved"] = true.into();
    data["filesystems"] = serde_json::json!(["fat32"]);
    stdfs::write(manifest, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(
        transfer(source.path(), dest.path())
            .unwrap()
            .completed
            .is_empty()
    );
    assert!(!dest.path().join("USB").exists());
}

#[test]
fn preexisting_usb_file_directory_and_dangling_symlink_are_all_refused() {
    let source = archive();
    game(source.path(), "a", 1);
    for variant in 0..3 {
        let dest = tempfile::tempdir().unwrap();
        let path = dest.path().join("USB");
        match variant {
            0 => stdfs::write(&path, b"personal").unwrap(),
            1 => {
                stdfs::create_dir(&path).unwrap();
                stdfs::write(path.join("mine"), b"personal").unwrap();
            }
            _ => symlink("missing", &path).unwrap(),
        }
        assert!(transfer(source.path(), dest.path()).is_err());
        match variant {
            0 => assert_eq!(stdfs::read(path).unwrap(), b"personal"),
            1 => assert_eq!(stdfs::read(path.join("mine")).unwrap(), b"personal"),
            _ => assert!(stdfs::symlink_metadata(path).unwrap().is_symlink()),
        }
    }
}

#[test]
fn roots_and_payload_symlinks_and_overlapping_directories_are_rejected() {
    let source = archive();
    game(source.path(), "a", 1);
    let dest = tempfile::tempdir().unwrap();
    let alias = dest.path().join("alias");
    symlink(source.path(), &alias).unwrap();
    assert!(transfer(&alias, dest.path()).is_err());
    assert!(transfer(source.path(), &alias).is_err());
    assert!(transfer(source.path(), source.path()).is_err());
    let inner = source.path().join("destination");
    stdfs::create_dir(&inner).unwrap();
    assert!(transfer(source.path(), &inner).is_err());
    assert!(transfer(source.path(), source.path().parent().unwrap()).is_err());
    let file = source.path().join("a/content/link");
    symlink("/etc/passwd", file).unwrap();
    assert!(transfer(source.path(), dest.path()).is_err());
    assert!(!dest.path().join("USB").exists());
}

#[test]
fn cancellation_before_writes_and_mid_game_never_publishes_partial_games() {
    let source = archive();
    game(source.path(), "a", 400_000);
    for early in [true, false] {
        let dest = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let report = copy_to_directory(source.path(), dest.path(), &options(), &cancel, |event| {
            if (early && matches!(event, Event::Prepared { .. }))
                || (!early && matches!(event, Event::CopyProgress { .. }))
            {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();
        assert_eq!(report.status, Status::Cancelled);
        assert!(report.completed.is_empty());
        assert!(!dest.path().join("USB/games/a").exists());
        if early {
            assert!(!dest.path().join("USB").exists());
        } else {
            assert!(dest.path().join("USB/.incoming/a/data.bin").is_file());
        }
    }
}

#[test]
fn cancellation_between_games_preserves_already_completed_games() {
    let source = archive();
    game(source.path(), "a", 10);
    game(source.path(), "b", 20);
    let dest = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let report = copy_to_directory(source.path(), dest.path(), &options(), &cancel, |event| {
        if matches!(event, Event::GameCompleted { .. }) {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .unwrap();
    assert_eq!(report.status, Status::Cancelled);
    assert_eq!(report.completed.len(), 1);
    assert!(dest.path().join("USB/games/a/data.bin").is_file());
    assert!(!dest.path().join("USB/games/b").exists());
}

#[test]
fn revoked_approval_and_same_size_replacement_are_detected_before_publication() {
    for revoke in [true, false] {
        let source = archive();
        game(source.path(), "a", 100);
        let dest = tempfile::tempdir().unwrap();
        let report = copy_to_directory(
            source.path(),
            dest.path(),
            &options(),
            &AtomicBool::new(false),
            |event| {
                if matches!(event, Event::Prepared { .. }) {
                    if revoke {
                        let manifest = source.path().join("a/game.json");
                        let mut data: serde_json::Value =
                            serde_json::from_slice(&stdfs::read(&manifest).unwrap()).unwrap();
                        data["redistribution_approved"] = false.into();
                        stdfs::write(manifest, serde_json::to_vec(&data).unwrap()).unwrap();
                    } else {
                        let file = source.path().join("a/content/data.bin");
                        stdfs::rename(&file, source.path().join("old-data")).unwrap();
                        stdfs::write(file, vec![0x5a; 100]).unwrap();
                    }
                }
            },
        )
        .unwrap();
        assert_eq!(report.status, Status::Failed);
        assert!(report.completed.is_empty());
        assert!(!dest.path().join("USB/games/a").exists());
    }
}

#[test]
fn source_mutation_and_approval_revocation_during_copy_abort_the_game() {
    for revoke in [true, false] {
        let source = archive();
        game(source.path(), "a", 400_000);
        let dest = tempfile::tempdir().unwrap();
        let mut changed = false;
        let report = copy_to_directory(
            source.path(),
            dest.path(),
            &options(),
            &AtomicBool::new(false),
            |event| {
                if matches!(event, Event::CopyProgress { .. }) && !changed {
                    changed = true;
                    let path = source.path().join(if revoke {
                        "a/game.json"
                    } else {
                        "a/content/data.bin"
                    });
                    if revoke {
                        let mut data: serde_json::Value =
                            serde_json::from_slice(&stdfs::read(&path).unwrap()).unwrap();
                        data["redistribution_approved"] = false.into();
                        stdfs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
                    } else {
                        let mut file = stdfs::OpenOptions::new().write(true).open(path).unwrap();
                        file.write_all(b"changed").unwrap();
                    }
                }
            },
        )
        .unwrap();
        assert_eq!(report.status, Status::Failed);
        assert!(report.completed.is_empty());
        assert!(!dest.path().join("USB/games/a").exists());
    }
}

#[test]
fn staged_corruption_deletion_and_injected_files_are_not_published() {
    for variant in 0..3 {
        let source = archive();
        game(source.path(), "a", 100);
        let dest = tempfile::tempdir().unwrap();
        let report = copy_to_directory(
            source.path(),
            dest.path(),
            &options(),
            &AtomicBool::new(false),
            |event| {
                if matches!(event, Event::GameVerified { .. }) {
                    let stage = dest.path().join("USB/.incoming/a");
                    match variant {
                        0 => stdfs::write(stage.join("data.bin"), vec![0x33; 100]).unwrap(),
                        1 => stdfs::remove_file(stage.join("data.bin")).unwrap(),
                        _ => stdfs::write(stage.join("surprise"), b"unexpected").unwrap(),
                    }
                }
            },
        )
        .unwrap();
        assert_eq!(report.status, Status::Failed);
        assert_eq!(report.failure.unwrap().kind, FailureKind::Checksum);
        assert!(!dest.path().join("USB/games/a").exists());
    }
}

#[test]
fn late_destination_collisions_never_replace_existing_content() {
    let source = archive();
    game(source.path(), "a", 100);
    let dest = tempfile::tempdir().unwrap();
    let report = copy_to_directory(
        source.path(),
        dest.path(),
        &options(),
        &AtomicBool::new(false),
        |event| {
            if matches!(event, Event::GameVerified { .. }) {
                let path = dest.path().join("USB/games/a");
                stdfs::create_dir(&path).unwrap();
                stdfs::write(path.join("personal"), b"untouched").unwrap();
            }
        },
    )
    .unwrap();
    assert_eq!(report.status, Status::Failed);
    assert!(report.completed.is_empty());
    assert_eq!(
        stdfs::read(dest.path().join("USB/games/a/personal")).unwrap(),
        b"untouched"
    );
    assert!(!dest.path().join("USB/games/a/data.bin").exists());
}

#[test]
fn replaced_destination_subdirectory_cannot_redirect_writes_outside_staging() {
    let source = archive();
    game(source.path(), "a", 100);
    let dest = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut changed = false;
    let report = copy_to_directory(
        source.path(),
        dest.path(),
        &options(),
        &AtomicBool::new(false),
        |event| {
            if matches!(event, Event::CopyProgress { .. }) && !changed {
                changed = true;
                let path = dest.path().join("USB/.incoming/a/extras");
                stdfs::rename(&path, dest.path().join("USB/.incoming/a/original-extras")).unwrap();
                symlink(outside.path(), path).unwrap();
            }
        },
    )
    .unwrap();
    assert_eq!(report.status, Status::Failed);
    assert_eq!(stdfs::read_dir(outside.path()).unwrap().count(), 0);
    assert!(!dest.path().join("USB/games/a").exists());
}

#[test]
fn shrinking_free_space_stops_copying_without_publishing() {
    let source = archive();
    game(source.path(), "a", 400_000);
    let dest = tempfile::tempdir().unwrap();
    let mut probes = 0;
    let report = copy_with_probe(
        source.path(),
        dest.path(),
        &options(),
        &AtomicBool::new(false),
        |_| {},
        |dir| {
            probes += 1;
            if probes >= 4 {
                Ok(Space {
                    available: 0,
                    unit: 4096,
                })
            } else {
                dir.space()
            }
        },
    )
    .unwrap();
    assert_eq!(report.status, Status::Failed);
    assert_eq!(report.failure.unwrap().kind, FailureKind::NoSpace);
    assert!(!dest.path().join("USB/games/a").exists());
}

#[test]
fn independent_transfers_isolate_cancellation_and_same_destination_is_locked() {
    let source = archive();
    game(source.path(), "a", 400_000);
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    std::thread::scope(|scope| {
        let one = scope.spawn(|| {
            let cancel = AtomicBool::new(false);
            copy_to_directory(source.path(), first.path(), &options(), &cancel, |event| {
                if matches!(event, Event::CopyProgress { .. }) {
                    cancel.store(true, Ordering::Relaxed);
                }
            })
            .unwrap()
        });
        let two = scope.spawn(|| transfer(source.path(), second.path()).unwrap());
        assert_eq!(one.join().unwrap().status, Status::Cancelled);
        assert_eq!(two.join().unwrap().status, Status::Completed);
    });
    let third = tempfile::tempdir().unwrap();
    let result = copy_to_directory(
        source.path(),
        third.path(),
        &options(),
        &AtomicBool::new(false),
        |event| {
            if matches!(event, Event::Prepared { .. }) {
                assert!(transfer(source.path(), third.path()).is_err());
            }
        },
    )
    .unwrap();
    assert_eq!(result.status, Status::Completed);
}

struct PartialWriter {
    bytes: Vec<u8>,
    fail_after: Option<usize>,
    interrupt_once: bool,
}
impl Write for PartialWriter {
    fn write(&mut self, data: &[u8]) -> stdio::Result<usize> {
        if self.interrupt_once {
            self.interrupt_once = false;
            return Err(stdio::ErrorKind::Interrupted.into());
        }
        if self
            .fail_after
            .is_some_and(|limit| self.bytes.len() >= limit)
        {
            return Err(stdio::Error::from_raw_os_error(28));
        }
        let count = data.len().min(3);
        self.bytes.extend_from_slice(&data[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> stdio::Result<()> {
        Ok(())
    }
}
#[test]
fn streaming_retries_short_writes_and_interrupted_writes_and_reports_enospc() {
    let mut writer = PartialWriter {
        bytes: Vec::new(),
        fail_after: None,
        interrupt_once: true,
    };
    let input = b"hello world";
    let hash = stream::copy_stream(
        &mut Cursor::new(input),
        &mut writer,
        input.len() as u64,
        &AtomicBool::new(false),
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(writer.bytes, input);
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    let mut writer = PartialWriter {
        bytes: Vec::new(),
        fail_after: Some(3),
        interrupt_once: false,
    };
    let error = stream::copy_stream(
        &mut Cursor::new(input),
        &mut writer,
        input.len() as u64,
        &AtomicBool::new(false),
        |_, _| Ok(()),
    )
    .unwrap_err();
    assert_eq!(Failure::from(error).kind, FailureKind::NoSpace);
}

#[test]
fn truncated_growing_unreadable_sources_and_zero_writes_fail() {
    let cancel = AtomicBool::new(false);
    for (data, expected) in [(b"abc".as_slice(), 4), (b"abcdef".as_slice(), 4)] {
        assert!(stream::hash(&mut Cursor::new(data), expected, &cancel).is_err());
    }
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> stdio::Result<usize> {
            Err(stdio::Error::from_raw_os_error(5))
        }
    }
    assert!(stream::hash(&mut BrokenReader, 10, &cancel).is_err());
    struct ZeroWriter;
    impl Write for ZeroWriter {
        fn write(&mut self, _: &[u8]) -> stdio::Result<usize> {
            Ok(0)
        }
        fn flush(&mut self) -> stdio::Result<()> {
            Ok(())
        }
    }
    assert!(
        stream::copy_stream(
            &mut Cursor::new(b"abc"),
            &mut ZeroWriter,
            3,
            &cancel,
            |_, _| Ok(())
        )
        .is_err()
    );
}

#[test]
fn invalid_options_fail_before_writes_and_measured_space_caps_the_budget() {
    let source = archive();
    game(source.path(), "a", 100);
    let dest = tempfile::tempdir().unwrap();
    let mut opts = options();
    opts.planner.allocation_unit = 513;
    assert!(
        copy_to_directory(
            source.path(),
            dest.path(),
            &opts,
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    assert!(!dest.path().join("USB").exists());
    let report = copy_with_probe(
        source.path(),
        dest.path(),
        &options(),
        &AtomicBool::new(false),
        |_| {},
        |_| {
            Ok(Space {
                available: 4096,
                unit: 4096,
            })
        },
    )
    .unwrap();
    assert!(report.plan.selected.is_empty());
    assert_eq!(report.measured_available_bytes, 4096);
    assert_eq!(report.plan.available_bytes, 0);
    assert!(!dest.path().join("USB").exists());
    let mut opts = options();
    opts.capacity_limit = Some(100_000);
    opts.planner.margin_bytes = 0;
    opts.planner.margin_percent = 10;
    let report = copy_with_probe(
        source.path(),
        dest.path(),
        &opts,
        &AtomicBool::new(false),
        |_| {},
        |_| {
            Ok(Space {
                available: 1_000_000,
                unit: 4096,
            })
        },
    )
    .unwrap();
    assert_eq!(report.plan.reserved_bytes, 10_000);
    assert_eq!(
        report.plan.available_bytes,
        100_000 - 4096 - report.journal_overhead_bytes
    );
    assert_eq!(report.status, Status::Completed);
}

#[test]
fn persisted_records_match_success_cancellation_and_failure_reports() {
    for outcome in ["success", "cancel", "failure"] {
        let source = archive();
        game(source.path(), "a", 400_000);
        let dest = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let report = copy_to_directory(source.path(), dest.path(), &options(), &cancel, |event| {
            if matches!(event, Event::GameVerified { .. }) {
                match outcome {
                    "cancel" => cancel.store(true, Ordering::Relaxed),
                    "failure" => {
                        stdfs::write(dest.path().join("USB/.incoming/a/data.bin"), b"changed")
                            .unwrap()
                    }
                    _ => {}
                }
            }
            // Inspection must refuse a destination while a transfer holds its lock.
            if matches!(event, Event::GameStarted { .. }) {
                assert!(inspect_transfer(dest.path()).is_err());
            }
        })
        .unwrap();
        let inspection = inspect_transfer(dest.path()).unwrap();
        assert_eq!(inspection.terminal_status, Some(report.status));
        assert_eq!(
            inspection.recorded_completed,
            report
                .completed
                .iter()
                .map(|g| g.id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(inspection.active_game, report.unconfirmed_game);
        assert!(!inspection.publication_uncertain);
    }
}

// Launched only by the parent crash test. exit() deliberately bypasses destructors
// and terminal-record handling, as an abruptly terminated copy process would.
#[test]
fn journal_crash_worker() {
    let Some(point) = std::env::var_os("USB_TEST_CRASH_POINT") else {
        return;
    };
    let source = std::env::var_os("USB_TEST_ARCHIVE").unwrap();
    let dest = std::env::var_os("USB_TEST_DESTINATION").unwrap();
    copy_to_directory(
        Path::new(&source),
        Path::new(&dest),
        &options(),
        &AtomicBool::new(false),
        |event| {
            let current = match event {
                Event::GameStarted { .. } => "started",
                Event::CopyProgress { .. } => "copying",
                Event::GameVerified { .. } => "verified",
                Event::GamePublished { .. } => "published",
                Event::GameCompleted { .. } => "committed",
                _ => return,
            };
            if point == current {
                std::process::exit(71);
            }
        },
    )
    .unwrap();
    panic!("crash point not reached");
}

#[test]
fn abrupt_process_exits_preserve_staging_and_publication_uncertainty() {
    for point in ["started", "copying", "verified", "published", "committed"] {
        let source = archive();
        game(source.path(), "a", 400_000);
        let dest = tempfile::tempdir().unwrap();
        stdfs::write(dest.path().join("personal.txt"), b"untouched").unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::journal_crash_worker", "--nocapture"])
            .env("USB_TEST_CRASH_POINT", point)
            .env("USB_TEST_ARCHIVE", source.path())
            .env("USB_TEST_DESTINATION", dest.path())
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(71),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        let inspection = inspect_transfer(dest.path()).unwrap();
        assert_eq!(inspection.terminal_status, None);
        assert_eq!(inspection.publication_uncertain, point == "published");
        assert_eq!(
            inspection.recorded_completed.len(),
            usize::from(point == "committed")
        );
        assert_eq!(
            inspection.active_game.as_deref(),
            if point == "committed" {
                None
            } else {
                Some("a")
            }
        );
        assert_eq!(
            dest.path().join("USB/games/a").exists(),
            matches!(point, "published" | "committed")
        );
        if matches!(point, "copying" | "verified") {
            assert!(dest.path().join("USB/.incoming/a/data.bin").exists());
        }
        let journal = dest.path().join("USB/transfer.jsonl");
        let before = stdfs::read(&journal).unwrap();
        assert!(transfer(source.path(), dest.path()).is_err());
        let fresh = tempfile::tempdir().unwrap();
        assert_eq!(
            transfer(source.path(), fresh.path()).unwrap().status,
            Status::Completed
        );
        assert_eq!(stdfs::read(journal).unwrap(), before);
        assert_eq!(
            stdfs::read(dest.path().join("personal.txt")).unwrap(),
            b"untouched"
        );
    }
}

#[test]
fn journal_rejects_malformed_records_and_unsafe_entries_without_changes() {
    let header = "{\"record\":\"prepared\",\"schema_version\":1,\"selected_games\":1}\n";
    let started = "{\"record\":\"started\",\"id\":\"a\"}\n";
    let publishing = "{\"record\":\"publishing\",\"id\":\"a\"}\n";
    let committed = "{\"record\":\"committed\",\"id\":\"a\"}\n";
    for text in [
        String::new(),
        header.replace("version\":1", "version\":2"),
        header.replace("version\":1", "version\":18446744073709551616"),
        header.replace("version\":1", "version\":1,\"extra\":0"),
        header.replace("version\":1", "version\":1,\"schema_version\":1"),
        format!("{header}{header}"),
        format!("{header}{started}{started}"),
        format!("{header}{started}{committed}"),
        format!("{header}{started}{publishing}{committed}{started}"),
        format!("{header}{{\"record\":\"finished\",\"status\":\"completed\"}}\n"),
        format!("{header}{{\"record\":\"started\",\"id\":\"../personal\"}}\n"),
        format!("{header}{{\"record\":\"unknown\"}}\n"),
        format!("{header}{started}{{\"record\":\"publishing\",\"id\":\"b\"}}\n"),
        format!(
            "{header}{started}{publishing}{committed}{{\"record\":\"finished\",\"status\":\"completed\"}}\n{started}"
        ),
        format!("{header}{}\n", " ".repeat(513)),
        header.trim_end().to_owned(),
        format!("{header}{{\"record\":\"started\""),
    ] {
        let dest = tempfile::tempdir().unwrap();
        stdfs::create_dir(dest.path().join("USB")).unwrap();
        let journal = dest.path().join("USB/transfer.jsonl");
        stdfs::write(&journal, &text).unwrap();
        assert!(inspect_transfer(dest.path()).is_err(), "accepted {text:?}");
        assert_eq!(stdfs::read_to_string(journal).unwrap(), text);
    }
    for directory in [false, true] {
        let dest = tempfile::tempdir().unwrap();
        stdfs::create_dir(dest.path().join("USB")).unwrap();
        let path = dest.path().join("USB/transfer.jsonl");
        if directory {
            stdfs::create_dir(path).unwrap();
        } else {
            symlink("missing", path).unwrap();
        }
        assert!(inspect_transfer(dest.path()).is_err());
    }
    assert!(matches!(
        journal::overhead(usize::MAX, 4096),
        Err(Error::Overflow)
    ));
    assert!(matches!(
        journal::overhead(1, u64::MAX),
        Err(Error::Overflow)
    ));
}

#[test]
fn late_failure_preserves_previous_games_and_reports_the_unconfirmed_game() {
    let source = archive();
    game(source.path(), "a", 10);
    game(source.path(), "b", 20);
    let dest = tempfile::tempdir().unwrap();
    let report = copy_to_directory(
        source.path(),
        dest.path(),
        &options(),
        &AtomicBool::new(false),
        |event| {
            if matches!(event, Event::GameStarted { id } if id == "b") {
                stdfs::remove_file(source.path().join("b/content/data.bin")).unwrap();
            }
        },
    )
    .unwrap();
    assert_eq!(report.status, Status::Failed);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.completed[0].id, "a");
    assert_eq!(report.unconfirmed_game.as_deref(), Some("b"));
    assert!(dest.path().join("USB/games/a/extras/LICENCE.txt").is_file());
    assert!(!dest.path().join("USB/games/b").exists());
}
