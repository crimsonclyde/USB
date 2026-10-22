use super::*;
use crate::faults::{Guard, Point};

fn fixture() -> TempDir {
    let source = archive();
    for (id, bytes) in [("a", 64), ("b", 128), ("c", 192)] {
        game(source.path(), id, bytes);
    }
    source
}

fn run(source: &Path, dest: &Path, event: impl FnMut(&Event)) -> Report {
    copy_to_directory(source, dest, &options(), &AtomicBool::new(false), event).unwrap()
}

fn assert_preserved(source: &Path, dest: &Path, report: &Report) {
    assert_eq!(
        stdfs::read(dest.join("personal.txt")).unwrap(),
        b"untouched"
    );
    for (id, bytes) in [("a", 64), ("b", 128), ("c", 192)] {
        assert_eq!(
            stdfs::read(source.join(id).join("content/data.bin")).unwrap(),
            vec![0x5a; bytes]
        );
        let published = dest.join("USB/games").join(id);
        // Even an unconfirmed game must have its whole payload if rename succeeded.
        if published.exists() {
            assert_eq!(
                stdfs::read(published.join("data.bin")).unwrap(),
                vec![0x5a; bytes]
            );
            assert_eq!(
                stdfs::read(published.join("extras/LICENCE.txt")).unwrap(),
                b"Generated test fixture; freely redistributable."
            );
            assert!(published.join("extras/empty").is_dir());
        }
    }
    for game in &report.completed {
        assert!(dest.join("USB/games").join(&game.id).is_dir());
        assert!(!dest.join("USB/.incoming").join(&game.id).exists());
    }
}

fn destination() -> TempDir {
    let dest = tempfile::tempdir().unwrap();
    stdfs::write(dest.path().join("personal.txt"), b"untouched").unwrap();
    dest
}

#[test]
fn every_journal_write_and_file_or_directory_sync_failure_stops_the_transfer() {
    let source = fixture();
    let baseline = destination();
    let trace = Guard::arm(Point::DirectorySync, usize::MAX, 0, 5);
    assert_eq!(
        run(source.path(), baseline.path(), |_| {}).status,
        Status::Completed
    );
    let calls = trace.calls();
    drop(trace);
    let mut retained_completed_game = false;
    let mut retained_unconfirmed_publication = false;
    for point in [
        Point::JournalWrite,
        Point::JournalSync,
        Point::DirectorySync,
        Point::PayloadSync,
    ] {
        let count = calls.iter().filter(|p| **p == point).count();
        assert!(count > 0, "fault point {point:?} was never reached");
        let modes: &[(usize, i32)] = if point == Point::JournalWrite {
            &[(0, 5), (7, 5), (0, 28), (7, 28)]
        } else {
            &[(0, 5)]
        };
        for &(prefix, errno) in modes {
            for occurrence in 1..=count {
                let dest = destination();
                let fault = Guard::arm(point, occurrence, prefix, errno);
                let mut completions = Vec::new();
                let mut finished = Vec::new();
                let report = run(source.path(), dest.path(), |event| match event {
                    Event::GameCompleted { id } => completions.push(id.clone()),
                    Event::Finished { status } => finished.push(*status),
                    _ => {}
                });
                assert!(fault.fired(), "{point:?} occurrence {occurrence}");
                assert_eq!(report.status, Status::Failed);
                assert_eq!(
                    report.failure.as_ref().unwrap().kind,
                    if errno == 28 {
                        FailureKind::NoSpace
                    } else {
                        FailureKind::Io
                    }
                );
                assert_eq!(finished, [Status::Failed]);
                assert_eq!(
                    completions,
                    report
                        .completed
                        .iter()
                        .map(|g| g.id.clone())
                        .collect::<Vec<_>>()
                );
                if let Some(id) = &report.unconfirmed_game {
                    assert!(!completions.contains(id));
                    retained_unconfirmed_publication |=
                        dest.path().join("USB/games").join(id).exists();
                }
                retained_completed_game |= !report.completed.is_empty();
                if matches!(point, Point::JournalWrite | Point::JournalSync) {
                    // A failed stream cannot acquire a terminal record or more games.
                    assert_eq!(fault.calls().last(), Some(&point));
                }
                drop(fault);
                assert_preserved(source.path(), dest.path(), &report);
                let before = stdfs::read(dest.path().join("USB/transfer.jsonl"));
                assert!(transfer(source.path(), dest.path()).is_err());
                assert_eq!(
                    stdfs::read(dest.path().join("USB/transfer.jsonl")).ok(),
                    before.ok()
                );
            }
        }
    }
    assert!(retained_completed_game);
    assert!(retained_unconfirmed_publication);
}

#[test]
fn successful_rename_followed_by_any_parent_sync_failure_stays_unconfirmed() {
    let source = fixture();
    // After verification: games/, .incoming/, USB/, then destination root.
    for occurrence in 1..=4 {
        let dest = destination();
        let mut fault = None;
        let mut published = Vec::new();
        let mut completed = Vec::new();
        let report = run(source.path(), dest.path(), |event| match event {
            Event::GameVerified { id } if id == "b" => {
                fault = Some(Guard::arm(Point::DirectorySync, occurrence, 0, 5));
            }
            Event::GamePublished { id } => published.push(id.clone()),
            Event::GameCompleted { id } => completed.push(id.clone()),
            _ => {}
        });
        assert!(fault.as_ref().unwrap().fired());
        drop(fault);
        assert_eq!(report.status, Status::Failed);
        assert_eq!(report.failure.as_ref().unwrap().kind, FailureKind::Io);
        assert_eq!(
            report
                .completed
                .iter()
                .map(|g| g.id.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(report.unconfirmed_game.as_deref(), Some("b"));
        assert_eq!(completed, ["a"]);
        assert_eq!(published, ["a"]);
        assert!(dest.path().join("USB/games/b").is_dir());
        assert!(!dest.path().join("USB/.incoming/b").exists());
        assert!(!dest.path().join("USB/games/c").exists());
        assert!(!dest.path().join("USB/.incoming/c").exists());
        assert_preserved(source.path(), dest.path(), &report);
        let inspection = inspect_transfer(dest.path()).unwrap();
        assert_eq!(inspection.recorded_completed, ["a"]);
        assert_eq!(inspection.active_game.as_deref(), Some("b"));
        assert!(inspection.publication_uncertain);
        assert_eq!(inspection.terminal_status, Some(Status::Failed));
        let fresh = destination();
        assert_eq!(
            run(source.path(), fresh.path(), |_| {}).status,
            Status::Completed
        );
        assert_preserved(source.path(), dest.path(), &report);
    }
}

#[test]
fn torn_commit_preserves_the_published_game_and_rejects_journal_recovery() {
    let source = fixture();
    let dest = destination();
    let mut fault = None;
    let report = run(source.path(), dest.path(), |event| {
        if matches!(event, Event::GamePublished { id } if id == "b") {
            fault = Some(Guard::arm(Point::JournalWrite, 1, 7, 28));
        }
    });
    assert!(fault.as_ref().unwrap().fired());
    drop(fault);
    assert_eq!(report.status, Status::Failed);
    assert_eq!(report.failure.as_ref().unwrap().kind, FailureKind::NoSpace);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.unconfirmed_game.as_deref(), Some("b"));
    assert!(dest.path().join("USB/games/b").is_dir());
    assert!(!dest.path().join("USB/games/c").exists());
    let bytes = stdfs::read(dest.path().join("USB/transfer.jsonl")).unwrap();
    assert!(bytes.ends_with(b"{\"recor"));
    assert!(inspect_transfer(dest.path()).is_err());
    assert_preserved(source.path(), dest.path(), &report);
}

#[test]
fn failed_commit_sync_can_leave_a_visible_record_but_never_confirms_delivery() {
    let source = fixture();
    let dest = destination();
    let mut fault = None;
    let mut completed = Vec::new();
    let report = run(source.path(), dest.path(), |event| {
        if matches!(event, Event::GamePublished { id } if id == "b") {
            fault = Some(Guard::arm(Point::JournalSync, 1, 0, 5));
        }
        if let Event::GameCompleted { id } = event {
            completed.push(id.clone());
        }
    });
    assert!(fault.as_ref().unwrap().fired());
    drop(fault);
    assert_eq!(report.status, Status::Failed);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(completed, ["a"]);
    assert_eq!(report.unconfirmed_game.as_deref(), Some("b"));
    assert_preserved(source.path(), dest.path(), &report);
    // Read-back is visibility, not proof that the preceding fsync succeeded.
    let inspection = inspect_transfer(dest.path()).unwrap();
    assert_eq!(inspection.recorded_completed, ["a", "b"]);
    assert_eq!(inspection.terminal_status, None);
    assert!(!dest.path().join("USB/games/c").exists());
}

#[test]
fn terminal_record_failure_preserves_all_previously_confirmed_games() {
    let source = fixture();
    for point in [Point::JournalWrite, Point::JournalSync] {
        let dest = destination();
        let mut fault = None;
        let mut finished = Vec::new();
        let report = run(source.path(), dest.path(), |event| {
            if matches!(event, Event::GameCompleted { id } if id == "c") {
                fault = Some(Guard::arm(point, 1, 7, 5));
            }
            if let Event::Finished { status } = event {
                finished.push(*status);
            }
        });
        assert!(fault.as_ref().unwrap().fired());
        drop(fault);
        assert_eq!(report.status, Status::Failed);
        assert_eq!(report.failure.as_ref().unwrap().kind, FailureKind::Io);
        assert_eq!(report.completed.len(), 3);
        assert!(report.unconfirmed_game.is_none());
        assert_eq!(finished, [Status::Failed]);
        assert_preserved(source.path(), dest.path(), &report);
        if point == Point::JournalWrite {
            assert!(inspect_transfer(dest.path()).is_err());
        } else {
            // The unsynced terminal line can be readable even though runtime failed.
            assert_eq!(
                inspect_transfer(dest.path()).unwrap().terminal_status,
                Some(Status::Completed)
            );
        }
    }
}

#[test]
fn faults_do_not_escape_the_test_thread_or_guard_lifetime() {
    let source = fixture();
    let first = destination();
    let second = destination();
    std::thread::scope(|scope| {
        let fault = Guard::arm(Point::JournalWrite, 1, 7, 5);
        let other = scope.spawn(|| run(source.path(), second.path(), |_| {}));
        assert_eq!(
            run(source.path(), first.path(), |_| {}).status,
            Status::Failed
        );
        assert!(fault.fired());
        assert_eq!(other.join().unwrap().status, Status::Completed);
    });
    let third = destination();
    assert_eq!(
        run(source.path(), third.path(), |_| {}).status,
        Status::Completed
    );
}
