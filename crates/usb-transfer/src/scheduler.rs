//! Bounded batches of independent directory transfers. No device management.
use crate::{
    Error, Event, Failure, FailureKind, Report, Result, Status, TransferOptions, copy_to_directory,
    fs::{Directory, io},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};

pub const MAX_JOBS: usize = 10;
pub const MAX_WORKERS: usize = 10;

pub struct Job {
    pub id: String,
    pub archive: PathBuf,
    pub destination: PathBuf,
    pub options: TransferOptions,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
pub struct JobResult {
    pub id: String,
    pub status: Status,
    pub started: bool,
    pub elapsed_millis: u128,
    pub report: Option<Report>,
    /// Preparation/worker failure only; runtime transfer failures live in report.
    pub failure: Option<Failure>,
}

#[derive(Debug, Serialize)]
pub struct BatchReport {
    pub status: Status,
    pub workers_started: usize,
    pub event_queue_capacity: usize,
    pub elapsed_millis: u128,
    pub confirmed_payload_bytes: u128,
    /// Always input order, irrespective of worker/event interleaving.
    pub jobs: Vec<JobResult>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JobEvent {
    Queued { job_id: String },
    Started { job_id: String },
    Transfer { job_id: String, transfer: Event },
    Finished { job_id: String, status: Status },
}

enum Message {
    Event(JobEvent),
    Result(usize, Box<JobResult>),
}

fn worker_failure(id: &str) -> JobResult {
    JobResult {
        id: id.into(),
        status: Status::Failed,
        started: true,
        elapsed_millis: 0,
        report: None,
        failure: Some(Failure {
            kind: FailureKind::Worker,
            message: "transfer worker stopped unexpectedly; retain the destination for inspection"
                .into(),
        }),
    }
}

fn execute(job: &Job, tx: &mpsc::SyncSender<Message>) -> JobResult {
    let start = Instant::now();
    let started = !job.cancel.load(Ordering::Relaxed);
    if !started {
        return JobResult {
            id: job.id.clone(),
            status: Status::Cancelled,
            started: false,
            elapsed_millis: 0,
            report: None,
            failure: Some(Error::Cancelled.into()),
        };
    }
    if tx
        .send(Message::Event(JobEvent::Started {
            job_id: job.id.clone(),
        }))
        .is_err()
    {
        job.cancel.store(true, Ordering::Relaxed);
    }
    let result = copy_to_directory(
        &job.archive,
        &job.destination,
        &job.options,
        &job.cancel,
        |event| {
            if tx
                .send(Message::Event(JobEvent::Transfer {
                    job_id: job.id.clone(),
                    transfer: event.clone(),
                }))
                .is_err()
            {
                job.cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    let mut outcome = JobResult {
        id: job.id.clone(),
        status: Status::Failed,
        started,
        elapsed_millis: start.elapsed().as_millis(),
        report: None,
        failure: None,
    };
    match result {
        Ok(report) => {
            outcome.status = report.status;
            outcome.report = Some(report);
        }
        Err(error) => {
            let failure = Failure::from(error);
            if failure.kind == FailureKind::Cancelled {
                outcome.status = Status::Cancelled;
            }
            outcome.failure = Some(failure);
        }
    }
    outcome
}

fn validate(jobs: &[Job], workers: usize) -> Result<()> {
    if !(1..=MAX_WORKERS).contains(&workers) || jobs.is_empty() || jobs.len() > MAX_JOBS {
        return Err(Error::Invalid(
            "batch requires 1–10 jobs and 1–10 workers".into(),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut destinations: Vec<(PathBuf, crate::fs::Identity)> = Vec::new();
    for job in jobs {
        if !usb_core::catalogue::valid_id(&job.id) || !ids.insert(&job.id) {
            return Err(Error::Invalid(
                "batch job IDs must be valid, unique stable IDs".into(),
            ));
        }
        job.options.planner.validate()?;
        // Invalid/missing roots are per-job preparation errors. For roots that
        // exist, reject aliases and nesting before any worker can write.
        if let Ok((directory, path)) = Directory::open_root(&job.destination) {
            let identity = directory.identity()?;
            for (other_path, other_identity) in &destinations {
                if identity.same_node(other_identity)
                    || path.starts_with(other_path)
                    || other_path.starts_with(&path)
                {
                    return Err(Error::Invalid(
                        "batch destinations must be separate, non-overlapping directories".into(),
                    ));
                }
            }
            destinations.push((path, identity));
        }
    }
    Ok(())
}

/// Run a finite batch, using up to `workers` threads and a queue of at most twice
/// that many messages. The caller handles serialized events; a slow callback
/// applies backpressure. Job results retain input order; events may interleave.
/// Cancelling one job does not cancel siblings. Set all job flags to stop a batch.
/// Global validation errors return before writes; per-job failures are isolated.
pub fn run_batch(
    jobs: &[Job],
    workers: usize,
    mut on_event: impl FnMut(&JobEvent),
) -> Result<BatchReport> {
    validate(jobs, workers)?;
    let start = Instant::now();
    let workers = workers.min(jobs.len());
    let capacity = workers.checked_mul(2).ok_or(Error::Overflow)?;
    for job in jobs {
        on_event(&JobEvent::Queued {
            job_id: job.id.clone(),
        });
    }
    let next = AtomicUsize::new(0);
    let (outcomes, workers_started) = std::thread::scope(|scope| -> Result<_> {
        // Own the receiver inside the scope callback so it is dropped before
        // joining workers if a user callback panics; blocked sends then disconnect.
        let (tx, rx) = mpsc::sync_channel(capacity);
        let mut handles = Vec::new();
        for number in 0..workers {
            let tx = tx.clone();
            let next = &next;
            let handle = std::thread::Builder::new()
                .name(format!("usb-copy-{number}"))
                .spawn_scoped(scope, move || {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else {
                            break;
                        };
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                execute(job, &tx)
                            }))
                            .unwrap_or_else(|_| worker_failure(&job.id));
                        if tx.send(Message::Result(index, Box::new(outcome))).is_err() {
                            break;
                        }
                    }
                });
            match handle {
                Ok(handle) => handles.push(handle),
                // The worker count is an upper bound. Already started workers
                // can drain the finite queue if a later spawn hits a host limit.
                Err(error) if handles.is_empty() => {
                    return io("start transfer worker", ".", Err(error));
                }
                Err(_) => break,
            }
        }
        drop(tx);
        let mut outcomes: Vec<Option<JobResult>> = (0..jobs.len()).map(|_| None).collect();
        for message in rx {
            match message {
                Message::Event(event) => on_event(&event),
                Message::Result(index, outcome) => {
                    on_event(&JobEvent::Finished {
                        job_id: outcome.id.clone(),
                        status: outcome.status,
                    });
                    outcomes[index] = Some(*outcome);
                }
            }
        }
        let workers_started = handles.len();
        for handle in handles {
            let _ = handle.join();
        }
        Ok((outcomes, workers_started))
    })?;
    let jobs: Vec<_> = outcomes
        .into_iter()
        .zip(jobs)
        .map(|(outcome, job)| outcome.unwrap_or_else(|| worker_failure(&job.id)))
        .collect();
    let status = if jobs.iter().any(|j| j.status == Status::Failed) {
        Status::Failed
    } else if jobs.iter().any(|j| j.status == Status::Cancelled) {
        Status::Cancelled
    } else {
        Status::Completed
    };
    let confirmed_payload_bytes = jobs
        .iter()
        .filter_map(|j| j.report.as_ref())
        .flat_map(|r| &r.completed)
        .try_fold(0_u128, |total, game| {
            total
                .checked_add(u128::from(game.payload_bytes))
                .ok_or(Error::Overflow)
        })?;
    Ok(BatchReport {
        status,
        workers_started,
        event_queue_capacity: capacity,
        elapsed_millis: start.elapsed().as_millis(),
        confirmed_payload_bytes,
        jobs,
    })
}
