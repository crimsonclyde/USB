//! Append-only diagnostics, never an ownership marker or a resume authorisation.
use crate::{
    Error, Result, Status,
    fs::{Directory, io},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

const NAME: &str = "transfer.jsonl";
const MAX_RECORD: u64 = 512;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Record {
    Prepared {
        schema_version: u32,
        selected_games: u64,
    },
    Started {
        id: String,
    },
    Publishing {
        id: String,
    },
    Committed {
        id: String,
    },
    Finished {
        status: Status,
    },
}

pub(crate) struct Journal {
    file: File,
    healthy: bool,
}
impl Journal {
    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    pub fn create(distribution: &Directory, selected_games: usize) -> Result<Self> {
        let mut journal = Self {
            file: distribution.create_file(Path::new(NAME))?,
            healthy: true,
        };
        journal.append(&Record::Prepared {
            schema_version: 1,
            selected_games: u64::try_from(selected_games).map_err(|_| Error::Overflow)?,
        })?;
        distribution.sync()?;
        Ok(journal)
    }

    pub fn append(&mut self, record: &Record) -> Result<()> {
        if !self.healthy {
            return Err(Error::Invalid(
                "transfer journal failed earlier; no further records written".into(),
            ));
        }
        let mut bytes = serde_json::to_vec(record)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_RECORD {
            return Err(Error::Invalid(
                "transfer record exceeds schema bound".into(),
            ));
        }
        self.healthy = false;
        #[cfg(test)]
        io(
            "write transfer record",
            NAME,
            crate::faults::before_journal_write(&mut self.file, &bytes),
        )?;
        io("write transfer record", NAME, self.file.write_all(&bytes))?;
        #[cfg(test)]
        io(
            "sync transfer record",
            NAME,
            crate::faults::check(crate::faults::Point::JournalSync),
        )?;
        io("sync transfer record", NAME, self.file.sync_all())?;
        self.healthy = true;
        Ok(())
    }
}

/// Worst-case allocation for three records per catalogue game, a header, a
/// terminal record and file metadata. Independent of which games the planner selects.
pub(crate) fn overhead(games: usize, unit: u64) -> Result<u64> {
    let records = u64::try_from(games)
        .map_err(|_| Error::Overflow)?
        .checked_mul(3)
        .and_then(|n| n.checked_add(2))
        .ok_or(Error::Overflow)?;
    records
        .checked_mul(MAX_RECORD)
        .and_then(|n| n.div_ceil(unit).checked_mul(unit))
        .and_then(|n| n.checked_add(unit))
        .ok_or(Error::Overflow)
}

/// Historical journal claims only. Payload is not read or reverified. A committed
/// record is not proof that its sync succeeded or that content is still present,
/// approved or Bureau-owned. Visible records can outlive a reported sync failure.
#[derive(Debug, Serialize)]
pub struct Inspection {
    pub schema_version: u32,
    pub selected_games: u64,
    pub recorded_completed: Vec<String>,
    pub active_game: Option<String>,
    pub publication_uncertain: bool,
    /// None means interrupted or unknown, including a crash before terminal sync.
    pub terminal_status: Option<Status>,
}

/// Read only USB/transfer.jsonl under a locked destination. Refuses malformed,
/// truncated, out-of-order or unknown records. Does not inspect personal contents,
/// modify the tree, establish ownership, resume, or authorise removal.
pub fn inspect_transfer(destination: &Path) -> Result<Inspection> {
    let (destination, _) = Directory::open_root(destination)?;
    destination.lock()?;
    let distribution = destination.open(Path::new("USB"))?;
    inspect(distribution.file(Path::new(NAME))?)
}

fn invalid() -> Error {
    Error::Invalid(
        "invalid or incomplete transfer journal; retain the tree for operator inspection".into(),
    )
}

fn inspect(file: File) -> Result<Inspection> {
    let mut reader = BufReader::new(file);
    let mut state: Option<Inspection> = None;
    let mut seen = BTreeSet::new();
    loop {
        let mut line = Vec::new();
        io(
            "read transfer record",
            NAME,
            reader
                .by_ref()
                .take(MAX_RECORD + 1)
                .read_until(b'\n', &mut line),
        )?;
        if line.is_empty() {
            break;
        }
        if line.len() as u64 > MAX_RECORD || line.last() != Some(&b'\n') {
            return Err(invalid());
        }
        let record: Record = serde_json::from_slice(&line)?;
        if let Record::Prepared {
            schema_version: 1,
            selected_games,
        } = record
        {
            if state.is_some() || selected_games == 0 {
                return Err(invalid());
            }
            state = Some(Inspection {
                schema_version: 1,
                selected_games,
                recorded_completed: Vec::new(),
                active_game: None,
                publication_uncertain: false,
                terminal_status: None,
            });
            continue;
        }
        let state = state.as_mut().ok_or_else(invalid)?;
        if state.terminal_status.is_some() {
            return Err(invalid());
        }
        match record {
            Record::Started { id }
                if usb_core::catalogue::valid_id(&id)
                    && state.active_game.is_none()
                    && (seen.len() as u64) < state.selected_games
                    && seen.insert(id.clone()) =>
            {
                state.active_game = Some(id);
            }
            Record::Publishing { id }
                if state.active_game.as_ref() == Some(&id) && !state.publication_uncertain =>
            {
                state.publication_uncertain = true;
            }
            Record::Committed { id }
                if state.active_game.as_ref() == Some(&id) && state.publication_uncertain =>
            {
                state.recorded_completed.push(id);
                state.active_game = None;
                state.publication_uncertain = false;
            }
            Record::Finished { status }
                if status != Status::Completed
                    || (state.active_game.is_none()
                        && state.recorded_completed.len() as u64 == state.selected_games) =>
            {
                state.terminal_status = Some(status);
            }
            _ => return Err(invalid()),
        }
    }
    state.ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_journal_writes_disable_later_appends() {
        let fixture = tempfile::NamedTempFile::new().unwrap();
        let mut journal = Journal {
            file: File::open(fixture.path()).unwrap(),
            healthy: true,
        };
        let record = Record::Finished {
            status: Status::Failed,
        };
        assert!(matches!(journal.append(&record), Err(Error::Io { .. })));
        assert!(!journal.is_healthy());
        assert!(matches!(journal.append(&record), Err(Error::Invalid(_))));
        assert_eq!(std::fs::read(fixture.path()).unwrap(), b"");
    }
}
