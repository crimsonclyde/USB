//! Greedy selection in priority, age, size, ID order; O(n log n), no backtracking.
use crate::{
    Error, Result,
    catalogue::{Catalogue, Game},
    compatibility::{Filesystem, incompatibility},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Priorities {
    pub platforms: BTreeMap<String, i32>,
    pub collections: BTreeMap<String, i32>,
    pub games: BTreeMap<String, i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub margin_bytes: u64,
    /// Percentage of supplied free space, rounded up. The larger margin wins.
    pub margin_percent: u8,
    /// Conservative estimate; set to at least the actual destination cluster size.
    pub allocation_unit: u64,
    pub priorities: Priorities,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            margin_bytes: 64 * 1024 * 1024,
            margin_percent: 2,
            allocation_unit: 32 * 1024,
            priorities: Priorities::default(),
        }
    }
}

impl Options {
    /// Validate budget settings independently of catalogue or destination I/O.
    pub fn validate(&self) -> Result<()> {
        if self.margin_percent > 100
            || !(512..=1024 * 1024).contains(&self.allocation_unit)
            || !self.allocation_unit.is_power_of_two()
        {
            return Err(Error::Invalid("margin must be 0..=100 percent; allocation unit must be a power of two from 512 to 1048576".into()));
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", content = "detail", rename_all = "snake_case")]
pub enum SkipReason {
    Unapproved,
    Incompatible(String),
    InsufficientCapacity { required: u64, remaining: u64 },
}

#[derive(Debug, Serialize)]
pub struct Skipped {
    pub id: String,
    pub name: String,
    pub reason: SkipReason,
}

#[derive(Debug, Serialize)]
pub struct Selected {
    pub id: String,
    pub name: String,
    pub destination: String,
    pub payload_bytes: u64,
    /// Includes estimated per-game overhead; shared directories are charged once.
    pub estimated_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub available_bytes: u64,
    pub reserved_bytes: u64,
    pub safe_capacity: u64,
    pub selected: Vec<Selected>,
    pub skipped: Vec<Skipped>,
    pub payload_bytes: u64,
    pub planned_bytes: u64,
    pub remaining_safe_bytes: u64,
    pub remaining_available_bytes: u64,
}

fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(Error::Overflow)
}

/// Includes file rounding, one metadata unit per file, and one unit per directory
/// (including each game root). This is an estimate, not a free-space guarantee.
fn estimate(game: &Game, unit: u64) -> Result<u64> {
    let mut bytes = u64::try_from(game.directories.len()).map_err(|_| Error::Overflow)?;
    bytes = add(bytes, 1)?.checked_mul(unit).ok_or(Error::Overflow)?;
    for file in &game.files {
        let rounded = file
            .bytes
            .div_ceil(unit)
            .checked_mul(unit)
            .ok_or(Error::Overflow)?;
        bytes = add(bytes, add(rounded, unit)?)?;
    }
    Ok(bytes)
}

fn priority(catalogue: &Catalogue, game: &Game, rules: &Priorities) -> i64 {
    let m = &game.metadata;
    i64::from(*rules.games.get(&m.id).unwrap_or(&m.priority))
        + i64::from(
            *rules
                .platforms
                .get(&m.platform)
                .unwrap_or(&catalogue.platforms[&m.platform].priority),
        )
        + i64::from(
            *rules
                .collections
                .get(&m.collection)
                .unwrap_or(&catalogue.collections[&m.collection].priority),
        )
}

pub fn plan(
    catalogue: &Catalogue,
    available: u64,
    filesystem: Filesystem,
    options: &Options,
) -> Result<Plan> {
    options.validate()?;
    // Validate references even for an in-memory catalogue supplied by another caller.
    let mut ids = std::collections::BTreeSet::new();
    for game in &catalogue.games {
        if !crate::catalogue::valid_id(&game.metadata.id)
            || !ids.insert(&game.metadata.id)
            || !catalogue.platforms.contains_key(&game.metadata.platform)
            || !catalogue
                .collections
                .contains_key(&game.metadata.collection)
        {
            return Err(Error::Invalid("invalid catalogue IDs or references".into()));
        }
        let actual_bytes = game
            .files
            .iter()
            .try_fold(0_u64, |sum, file| add(sum, file.bytes))?;
        if actual_bytes != game.bytes || game.files.is_empty() {
            return Err(Error::Invalid("invalid game payload totals".into()));
        }
    }
    if options
        .priorities
        .platforms
        .keys()
        .any(|k| !catalogue.platforms.contains_key(k))
        || options
            .priorities
            .collections
            .keys()
            .any(|k| !catalogue.collections.contains_key(k))
        || options.priorities.games.keys().any(|k| !ids.contains(k))
    {
        return Err(Error::Invalid(
            "priority override references unknown ID".into(),
        ));
    }
    let percentage =
        (u128::from(available) * u128::from(options.margin_percent)).div_ceil(100) as u64;
    let reserved = options.margin_bytes.max(percentage).min(available);
    let safe = available - reserved;
    let mut result = Plan {
        available_bytes: available,
        reserved_bytes: reserved,
        safe_capacity: safe,
        selected: Vec::new(),
        skipped: Vec::new(),
        payload_bytes: 0,
        planned_bytes: 0,
        remaining_safe_bytes: safe,
        remaining_available_bytes: available,
    };
    let mut ordered: Vec<_> = catalogue.games.iter().collect();
    ordered.sort_by(|a, b| {
        priority(catalogue, b, &options.priorities)
            .cmp(&priority(catalogue, a, &options.priorities))
            .then_with(|| {
                let year_key = |game: &Game| {
                    (
                        game.metadata.release_year.is_none(),
                        game.metadata.release_year.unwrap_or_default(),
                    )
                };
                year_key(a).cmp(&year_key(b))
            })
            .then_with(|| a.bytes.cmp(&b.bytes))
            .then_with(|| a.metadata.id.cmp(&b.metadata.id))
    });
    for game in ordered {
        let m = &game.metadata;
        let reason = if !m.redistribution_approved
            || m.approval_note.as_ref().is_none_or(|s| s.trim().is_empty())
        {
            Some(SkipReason::Unapproved)
        } else if let Some(reason) = incompatibility(game, filesystem) {
            Some(SkipReason::Incompatible(reason))
        } else {
            let estimated = estimate(game, options.allocation_unit)?;
            let shared = if result.selected.is_empty() {
                options.allocation_unit * 2
            } else {
                0
            };
            let required = add(estimated, shared)?;
            if required > result.remaining_safe_bytes {
                Some(SkipReason::InsufficientCapacity {
                    required,
                    remaining: result.remaining_safe_bytes,
                })
            } else {
                result.planned_bytes = add(result.planned_bytes, required)?;
                result.payload_bytes = add(result.payload_bytes, game.bytes)?;
                result.remaining_safe_bytes -= required;
                result.selected.push(Selected {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    destination: format!("USB/games/{}", m.id),
                    payload_bytes: game.bytes,
                    estimated_bytes: estimated,
                });
                None
            }
        };
        if let Some(reason) = reason {
            result.skipped.push(Skipped {
                id: m.id.clone(),
                name: m.name.clone(),
                reason,
            });
        }
    }
    result.remaining_available_bytes = available - result.planned_bytes;
    Ok(result)
}
