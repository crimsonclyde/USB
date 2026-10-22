use super::*;
use crate::scheduler::{Job, JobEvent, run_batch};
use std::sync::Arc;

fn job(source: &Path, destination: &Path, index: usize) -> Job {
    Job {
        id: format!("port-{index}"),
        archive: source.into(),
        destination: destination.into(),
        options: options(),
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn ten_destinations_obey_worker_limit_and_report_in_input_order() {
    let source = archive();
    game(source.path(), "a", 400_000);
    let destinations: Vec<_> = (0..10).map(|_| tempfile::tempdir().unwrap()).collect();
    let jobs: Vec<_> = destinations
        .iter()
        .enumerate()
        .map(|(i, d)| job(source.path(), d.path(), i))
        .collect();
    let mut active = 0;
    let mut peak = 0;
    let mut queued = Vec::new();
    let report = run_batch(&jobs, 3, |event| match event {
        JobEvent::Queued { job_id } => queued.push(job_id.clone()),
        JobEvent::Started { .. } => {
            active += 1;
            peak = peak.max(active);
        }
        JobEvent::Finished { .. } => {
            assert!(active > 0);
            active -= 1;
        }
        _ => {}
    })
    .unwrap();
    assert_eq!(report.status, Status::Completed);
    assert_eq!(report.workers_started, 3);
    assert_eq!(report.event_queue_capacity, 6);
    assert_eq!(active, 0);
    assert!((1..=3).contains(&peak));
    assert_eq!(
        queued,
        jobs.iter().map(|j| j.id.clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        report.jobs.iter().map(|j| &j.id).collect::<Vec<_>>(),
        jobs.iter().map(|j| &j.id).collect::<Vec<_>>()
    );
    assert_eq!(
        report.confirmed_payload_bytes,
        10 * (400_000 + b"Generated test fixture; freely redistributable.".len() as u128)
    );
    for (result, destination) in report.jobs.iter().zip(destinations) {
        assert!(result.started);
        assert_eq!(result.report.as_ref().unwrap().completed.len(), 1);
        assert_eq!(
            stdfs::read(destination.path().join("USB/games/a/data.bin")).unwrap(),
            vec![0x5a; 400_000]
        );
    }
}

#[test]
fn queued_and_running_cancellation_leave_other_jobs_independent() {
    let source = archive();
    game(source.path(), "a", 400_000);
    let destinations: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let jobs: Vec<_> = destinations
        .iter()
        .enumerate()
        .map(|(i, d)| job(source.path(), d.path(), i))
        .collect();
    jobs[2].cancel.store(true, Ordering::Relaxed);
    let mut started = Vec::new();
    let report = run_batch(&jobs, 1, |event| {
        if matches!(event, JobEvent::Transfer { job_id, transfer: Event::CopyProgress { .. } } if job_id == "port-0") {
            jobs[0].cancel.store(true, Ordering::Relaxed);
        }
        if let JobEvent::Started { job_id } = event { started.push(job_id.clone()); }
    }).unwrap();
    assert_eq!(report.status, Status::Cancelled);
    assert_eq!(
        report.jobs.iter().map(|j| j.status).collect::<Vec<_>>(),
        [Status::Cancelled, Status::Completed, Status::Cancelled]
    );
    assert_eq!(started, ["port-0", "port-1"]);
    assert!(!report.jobs[2].started);
    assert!(report.jobs[2].report.is_none());
    assert!(!destinations[0].path().join("USB/games/a").exists());
    assert!(destinations[0].path().join("USB/.incoming/a").exists());
    assert!(destinations[1].path().join("USB/games/a").is_dir());
    assert!(!destinations[2].path().join("USB").exists());
}

#[test]
fn bad_source_and_destination_fail_without_stopping_sibling_jobs() {
    let source = archive();
    game(source.path(), "a", 100);
    let destinations: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let mut jobs: Vec<_> = destinations
        .iter()
        .enumerate()
        .map(|(i, d)| job(source.path(), d.path(), i))
        .collect();
    jobs[0].archive = source.path().join("missing");
    stdfs::write(destinations[1].path().join("USB"), b"personal").unwrap();
    let report = run_batch(&jobs, 2, |_| {}).unwrap();
    assert_eq!(report.status, Status::Failed);
    assert_eq!(
        report.jobs.iter().map(|j| j.status).collect::<Vec<_>>(),
        [Status::Failed, Status::Failed, Status::Completed]
    );
    assert!(report.jobs[0].failure.is_some());
    assert!(report.jobs[1].failure.is_some());
    assert!(report.jobs[2].failure.is_none());
    assert_eq!(
        stdfs::read(destinations[1].path().join("USB")).unwrap(),
        b"personal"
    );
    assert!(destinations[2].path().join("USB/games/a").is_dir());
}

#[test]
fn invalid_batches_aliases_and_nested_destinations_fail_before_any_writes() {
    let source = archive();
    game(source.path(), "a", 100);
    let destination = tempfile::tempdir().unwrap();
    let jobs = [job(source.path(), destination.path(), 1)];
    for workers in [0, 11, usize::MAX] {
        assert!(run_batch(&jobs, workers, |_| {}).is_err());
    }
    assert!(run_batch(&[], 1, |_| {}).is_err());
    let too_many: Vec<_> = (0..11)
        .map(|i| job(source.path(), destination.path(), i))
        .collect();
    assert!(run_batch(&too_many, 1, |_| {}).is_err());
    let child = destination.path().join("child");
    stdfs::create_dir(&child).unwrap();
    for other in [destination.path().join("."), child] {
        let jobs = [
            job(source.path(), destination.path(), 1),
            job(source.path(), &other, 2),
        ];
        assert!(run_batch(&jobs, 2, |_| {}).is_err());
    }
    for id in ["port-1", "../bad"] {
        let other = tempfile::tempdir().unwrap();
        let mut jobs = [
            job(source.path(), destination.path(), 1),
            job(source.path(), other.path(), 2),
        ];
        jobs[1].id = id.into();
        assert!(run_batch(&jobs, 2, |_| {}).is_err());
        assert!(!other.path().join("USB").exists());
    }
    assert!(!destination.path().join("USB").exists());
}

#[test]
fn observer_panic_disconnects_workers_without_deadlock() {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let source = archive();
        game(source.path(), "a", 400_000);
        let destinations: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let jobs: Vec<_> = destinations
            .iter()
            .enumerate()
            .map(|(i, d)| job(source.path(), d.path(), i))
            .collect();
        let panic = std::panic::catch_unwind(|| {
            run_batch(&jobs, 2, |event| {
                if matches!(event, JobEvent::Started { .. }) {
                    panic!("observer stopped");
                }
            })
        });
        tx.send(panic.is_err()).unwrap();
    });
    assert!(rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap());
    worker.join().unwrap();
}

#[test]
fn queued_jobs_recheck_approval_after_an_earlier_transfer_stops() {
    let source = archive();
    game(source.path(), "a", 400_000);
    let destinations: Vec<_> = (0..2).map(|_| tempfile::tempdir().unwrap()).collect();
    let jobs: Vec<_> = destinations
        .iter()
        .enumerate()
        .map(|(i, d)| job(source.path(), d.path(), i))
        .collect();
    let mut revoked = false;
    let report = run_batch(&jobs, 1, |event| {
        if matches!(event, JobEvent::Transfer { job_id, transfer: Event::CopyProgress { .. } } if job_id == "port-0") && !revoked {
            revoked = true;
            let manifest = source.path().join("a/game.json");
            let mut data: serde_json::Value = serde_json::from_slice(&stdfs::read(&manifest).unwrap()).unwrap();
            data["redistribution_approved"] = false.into();
            stdfs::write(manifest, serde_json::to_vec(&data).unwrap()).unwrap();
        }
    }).unwrap();
    assert!(revoked);
    assert_eq!(report.jobs[0].status, Status::Failed);
    assert_eq!(report.jobs[1].status, Status::Completed);
    let second = report.jobs[1].report.as_ref().unwrap();
    assert!(second.plan.selected.is_empty());
    assert!(second.completed.is_empty());
    assert_eq!(report.confirmed_payload_bytes, 0);
    for dest in destinations {
        assert!(!dest.path().join("USB/games/a").exists());
    }
}
