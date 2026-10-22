//! Experimental Linux directory transfers. Games are staged, verified and published
//! whole. No USB APIs, mounting, automatic cleanup, overwriting or safe-removal claims.
mod fs;
mod journal;
pub mod scheduler;
mod snapshot;
mod stream;

#[cfg(test)]
mod faults;

use fs::{Directory, Space, io};
pub use journal::{Inspection, inspect_transfer};
use serde::{Deserialize, Serialize};
use snapshot::{Manifest, Snapshot, verify_identity};
use std::{
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use usb_core::{
    catalogue,
    compatibility::Filesystem,
    planner::{self, Options, Plan},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{operation} {path:?}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}")]
    Invalid(String),
    #[error("source changed: {0:?}")]
    SourceChanged(PathBuf),
    #[error("checksum verification failed: {0:?}")]
    Checksum(PathBuf),
    #[error("insufficient free space to preserve the safety reserve")]
    NoSpace,
    #[error("transfer cancelled")]
    Cancelled,
    #[error("size arithmetic overflow")]
    Overflow,
    #[error(transparent)]
    Catalogue(#[from] usb_core::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct TransferOptions {
    pub filesystem: Filesystem,
    pub planner: Options,
    /// Optional upper bound for this transfer; never replaces measured free space.
    pub capacity_limit: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Worker,
    Cancelled,
    NoSpace,
    SourceChanged,
    Checksum,
    Io,
    Invalid,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        let kind = match &error {
            Error::Cancelled => FailureKind::Cancelled,
            Error::NoSpace => FailureKind::NoSpace,
            Error::SourceChanged(_) => FailureKind::SourceChanged,
            Error::Checksum(_) => FailureKind::Checksum,
            Error::Io { source, .. } if source.kind() == std::io::ErrorKind::StorageFull => {
                FailureKind::NoSpace
            }
            Error::Io { .. } => FailureKind::Io,
            _ => FailureKind::Invalid,
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VerifiedFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub struct CompletedGame {
    pub id: String,
    pub payload_bytes: u64,
    pub files: Vec<VerifiedFile>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub status: Status,
    pub measured_available_bytes: u64,
    pub allocation_unit: u64,
    pub staging_overhead_bytes: u64,
    pub journal_overhead_bytes: u64,
    pub plan: Plan,
    pub completed: Vec<CompletedGame>,
    pub failure: Option<Failure>,
    /// May be in staging or already published if final directory syncing failed.
    pub unconfirmed_game: Option<String>,
    /// Records a creation attempt, not ownership or permission to clean anything.
    pub destination_write_attempted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Prepared {
        games: usize,
        payload_bytes: u64,
    },
    GameStarted {
        id: String,
    },
    CopyProgress {
        id: String,
        path: String,
        file_bytes: u64,
        total_bytes: u64,
    },
    GameVerified {
        id: String,
    },
    GameCompleted {
        id: String,
    },
    /// Rename and directory sync succeeded; durable journal commit is still pending.
    GamePublished {
        id: String,
    },
    Finished {
        status: Status,
    },
}

pub(crate) fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    Ok(())
}
fn sum(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(Error::Overflow)
}
fn ensure_space(space: Space, required: u64) -> Result<()> {
    if space.available < required {
        return Err(Error::NoSpace);
    }
    Ok(())
}

/// Copy a freshly scanned selection to a pre-existing local directory. Preparation
/// errors return Err before destination writes. Runtime failures return a Report
/// retaining completed games and diagnostic state. Existing USB entries are refused.
/// Callers may run independent transfers concurrently; one destination is locked.
pub fn copy_to_directory(
    archive: &Path,
    destination: &Path,
    options: &TransferOptions,
    cancel: &AtomicBool,
    on_event: impl FnMut(&Event),
) -> Result<Report> {
    copy_with_probe(
        archive,
        destination,
        options,
        cancel,
        on_event,
        Directory::space,
    )
}

fn copy_with_probe(
    archive: &Path,
    destination: &Path,
    options: &TransferOptions,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(&Event),
    mut space: impl FnMut(&Directory) -> Result<Space>,
) -> Result<Report> {
    options.planner.validate()?;
    cancelled(cancel)?;
    let (source, source_path) = Directory::open_root(archive)?;
    let (destination, destination_path) = Directory::open_root(destination)?;
    fs::reject_overlap(&source, &destination, &source_path, &destination_path)?;
    destination.lock()?;
    if destination.exists(Path::new("USB"))? {
        return Err(Error::Invalid("destination already contains USB; existing data is never merged, replaced or cleaned automatically".into()));
    }
    let (archive_manifest, _) = Manifest::capture(&source, Path::new("archive.json"))?;
    let catalogue = catalogue::scan(&source_path)?;
    let (current_root, _) = Directory::open_root(&source_path)?;
    if !current_root.identity()?.same_node(&source.identity()?) {
        return Err(Error::SourceChanged(source_path));
    }
    archive_manifest.verify(&source)?;
    let measured = space(&destination)?;
    let mut planner_options = options.planner.clone();
    planner_options.allocation_unit = planner_options.allocation_unit.max(measured.unit);
    let unit = planner_options.allocation_unit;
    // The planner accounts for USB/games; reserve .incoming and bounded journal I/O.
    let journal_overhead = journal::overhead(catalogue.games.len(), unit)?;
    let overhead = sum(unit, journal_overhead)?;
    let budget = measured
        .available
        .min(options.capacity_limit.unwrap_or(u64::MAX));
    // Deduct staging overhead without reducing the configured percentage reserve.
    let percentage_reserve =
        (u128::from(budget) * u128::from(planner_options.margin_percent)).div_ceil(100) as u64;
    planner_options.margin_bytes = planner_options.margin_bytes.max(percentage_reserve);
    let plan = planner::plan(
        &catalogue,
        budget.saturating_sub(overhead),
        options.filesystem,
        &planner_options,
    )?;
    let mut snapshots = Vec::new();
    for selected in &plan.selected {
        cancelled(cancel)?;
        let index = catalogue
            .games
            .binary_search_by(|g| g.metadata.id.cmp(&selected.id))
            .map_err(|_| Error::Invalid("selected game is absent from catalogue".into()))?;
        let game = &catalogue.games[index];
        snapshots.push(Snapshot::capture(&source, &catalogue, game, cancel)?);
    }
    archive_manifest.verify(&source)?;
    let mut report = Report {
        status: Status::Completed,
        measured_available_bytes: measured.available,
        allocation_unit: unit,
        staging_overhead_bytes: unit,
        journal_overhead_bytes: journal_overhead,
        plan,
        completed: Vec::new(),
        failure: None,
        unconfirmed_game: None,
        destination_write_attempted: false,
    };
    on_event(&Event::Prepared {
        games: report.plan.selected.len(),
        payload_bytes: report.plan.payload_bytes,
    });
    let mut journal = None;
    let execution = (|| -> Result<()> {
        cancelled(cancel)?;
        if snapshots.is_empty() {
            return Ok(());
        }
        ensure_space(
            space(&destination)?,
            sum(
                sum(report.plan.planned_bytes, report.plan.reserved_bytes)?,
                overhead,
            )?,
        )?;
        archive_manifest.verify(&source)?;
        report.destination_write_attempted = true;
        let distribution = destination.create_directory(Path::new("USB"))?;
        journal = Some(journal::Journal::create(&distribution, snapshots.len())?);
        let journal = journal
            .as_mut()
            .ok_or_else(|| Error::Invalid("missing transfer journal".into()))?;
        let games = distribution.create_directory(Path::new("games"))?;
        let incoming = distribution.create_directory(Path::new(".incoming"))?;
        let mut total_copied = 0_u64;
        for (snapshot, selected) in snapshots.iter().zip(&report.plan.selected) {
            cancelled(cancel)?;
            let id = &snapshot.game.metadata.id;
            report.unconfirmed_game = Some(id.clone());
            journal.append(&journal::Record::Started { id: id.clone() })?;
            archive_manifest.verify(&source)?;
            let content = snapshot.verify(&source)?;
            ensure_space(
                space(&destination)?,
                sum(selected.estimated_bytes, report.plan.reserved_bytes)?,
            )?;
            on_event(&Event::GameStarted { id: id.clone() });
            cancelled(cancel)?;
            let stage = incoming.create_directory(Path::new(id))?;
            for path in &snapshot.game.directories {
                stage.create_directory(Path::new(path))?;
            }
            let mut staged_files = Vec::new();
            for file in &snapshot.files {
                cancelled(cancel)?;
                let mut input = content.file(Path::new(&file.path))?;
                verify_identity(&input, &file.path, &file.identity)?;
                let mut output = stage.create_file(Path::new(&file.path))?;
                let digest = stream::copy_stream(
                    &mut input,
                    &mut output,
                    file.identity.bytes,
                    cancel,
                    |copied, upcoming| {
                        if upcoming > 0 {
                            let rounded = (upcoming as u64)
                                .div_ceil(unit)
                                .checked_mul(unit)
                                .ok_or(Error::Overflow)?;
                            ensure_space(
                                space(&destination)?,
                                sum(sum(rounded, unit)?, report.plan.reserved_bytes)?,
                            )?;
                        } else {
                            on_event(&Event::CopyProgress {
                                id: id.clone(),
                                path: file.path.clone(),
                                file_bytes: copied,
                                total_bytes: sum(total_copied, copied)?,
                            });
                        }
                        Ok(())
                    },
                )?;
                if digest != file.digest {
                    return Err(Error::SourceChanged(file.path.clone().into()));
                }
                verify_identity(&input, &file.path, &file.identity)?;
                // Strip special permission bits; retain only source executable bits.
                io(
                    "set payload permissions",
                    &file.path,
                    rustix::fs::fchmod(
                        &output,
                        rustix::fs::Mode::from_raw_mode(0o644 | (file.identity.mode & 0o111)),
                    )
                    .map_err(Into::into),
                )?;
                #[cfg(test)]
                io(
                    "sync payload",
                    &file.path,
                    faults::check(faults::Point::PayloadSync),
                )?;
                io("sync payload", &file.path, output.sync_all())?;
                staged_files.push((
                    file.path.clone(),
                    fs::Identity::of(io("stat staged payload", &file.path, output.metadata())?),
                ));
                total_copied = sum(total_copied, file.identity.bytes)?;
            }
            for path in snapshot.game.directories.iter().rev() {
                stage.open(Path::new(path))?.sync()?;
            }
            stage.sync()?;
            let staged_tree = snapshot::walk(&stage)?;
            if staged_tree.files != staged_files
                || staged_tree
                    .directories
                    .iter()
                    .filter(|(p, _)| !p.is_empty())
                    .map(|(p, _)| p)
                    .ne(snapshot.game.directories.iter())
            {
                return Err(Error::Checksum("staged tree changed".into()));
            }
            for (file, (_, identity)) in snapshot.files.iter().zip(&staged_files) {
                let mut staged = stage.file(Path::new(&file.path))?;
                verify_identity(&staged, &file.path, identity)?;
                io(
                    "rewind staged payload",
                    &file.path,
                    staged.seek(SeekFrom::Start(0)),
                )?;
                if stream::hash(&mut staged, file.identity.bytes, cancel)? != file.digest {
                    return Err(Error::Checksum(file.path.clone().into()));
                }
                verify_identity(&staged, &file.path, identity)?;
            }
            on_event(&Event::GameVerified { id: id.clone() });
            cancelled(cancel)?;
            archive_manifest.verify(&source)?;
            snapshot.verify(&source)?;
            if snapshot::walk(&stage)? != staged_tree
                || !incoming
                    .open(Path::new(id))?
                    .identity()?
                    .same_node(&stage.identity()?)
            {
                return Err(Error::Checksum(
                    "staged tree changed before publication".into(),
                ));
            }
            ensure_space(space(&destination)?, sum(report.plan.reserved_bytes, unit)?)?;
            journal.append(&journal::Record::Publishing { id: id.clone() })?;
            incoming.publish(id, &games)?;
            distribution.sync()?;
            destination.sync()?;
            on_event(&Event::GamePublished { id: id.clone() });
            journal.append(&journal::Record::Committed { id: id.clone() })?;
            report.completed.push(CompletedGame {
                id: id.clone(),
                payload_bytes: snapshot.game.bytes,
                files: snapshot
                    .files
                    .iter()
                    .map(|file| VerifiedFile {
                        path: file.path.clone(),
                        bytes: file.identity.bytes,
                        sha256: file.digest.clone(),
                    })
                    .collect(),
            });
            report.unconfirmed_game = None;
            on_event(&Event::GameCompleted { id: id.clone() });
        }
        Ok(())
    })();
    if let Err(error) = execution {
        let failure = Failure::from(error);
        report.status = if failure.kind == FailureKind::Cancelled {
            Status::Cancelled
        } else {
            Status::Failed
        };
        report.failure = Some(failure);
    }
    if let Some(journal) = journal.as_mut()
        && journal.is_healthy()
        && let Err(error) = journal.append(&journal::Record::Finished {
            status: report.status,
        })
    {
        report.status = Status::Failed;
        report.failure = Some(Failure::from(error));
    }
    on_event(&Event::Finished {
        status: report.status,
    });
    Ok(report)
}

#[cfg(test)]
mod tests;
