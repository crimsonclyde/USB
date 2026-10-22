use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use std::{error::Error, fs, io::Read, path::PathBuf};
use usb_core::{
    catalogue,
    compatibility::Filesystem,
    planner::{self, Options},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(
    version,
    about = "USB — The Unspeakable Storage Bureau. Catalogue, planning and experimental directory copying."
)]
struct Cli {
    /// JSON configuration file. CLI options override configuration values.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Archive directory (default: /srv/game-archive).
    #[arg(long, global = true)]
    archive: Option<PathBuf>,
    /// Emit structured JSON instead of human-readable output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read-only device metadata. Candidates require review; no mount/write permission.
    Devices {
        /// Inspect saved explicit-column lsblk JSON instead of accessing host metadata.
        #[arg(long)]
        snapshot: Option<PathBuf>,
    },
    /// Copy to up to ten independent local directories with bounded concurrency.
    CopyBatch {
        #[arg(long = "destination", required = true)]
        destinations: Vec<PathBuf>,
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(1..=10))]
        workers: u8,
        #[arg(long, value_parser = parse_filesystem)]
        filesystem: Filesystem,
        #[arg(long, value_parser = parse_bytes)]
        capacity_limit: Option<u64>,
        #[command(flatten)]
        budget: BudgetArgs,
    },
    /// Read historical transfer records without changing or resuming a USB tree.
    InspectTransfer {
        #[arg(long)]
        destination: PathBuf,
    },
    Catalogue {
        #[command(subcommand)]
        command: CatalogueCommand,
    },
    /// Estimate a distribution. Capacity means FREE bytes, not device size.
    Plan {
        #[arg(long, value_parser = parse_bytes)]
        capacity: u64,
        #[arg(long, value_parser = parse_filesystem)]
        filesystem: Filesystem,
        #[command(flatten)]
        budget: BudgetArgs,
    },
    /// Copy complete games into a fresh USB folder in an existing local directory.
    /// Experimental: does not mount devices or authorise safe removal.
    CopyToDirectory {
        #[arg(long)]
        destination: PathBuf,
        #[arg(long, value_parser = parse_filesystem)]
        filesystem: Filesystem,
        /// Upper bound on the measured free-space budget.
        #[arg(long, value_parser = parse_bytes)]
        capacity_limit: Option<u64>,
        #[command(flatten)]
        budget: BudgetArgs,
    },
}

#[derive(Args)]
struct BudgetArgs {
    #[arg(long, value_parser = parse_bytes)]
    margin_bytes: Option<u64>,
    #[arg(long)]
    margin_percent: Option<u8>,
    #[arg(long, value_parser = parse_bytes)]
    allocation_unit: Option<u64>,
}
impl BudgetArgs {
    fn apply(self, options: &mut Options) {
        if let Some(value) = self.margin_bytes {
            options.margin_bytes = value;
        }
        if let Some(value) = self.margin_percent {
            options.margin_percent = value;
        }
        if let Some(value) = self.allocation_unit {
            options.allocation_unit = value;
        }
    }
}

#[derive(Subcommand)]
enum CatalogueCommand {
    /// Inspect an archive. Nothing is cached or written.
    Scan { path: Option<PathBuf> },
    /// Rescan the configured archive and list its games.
    List,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    archive: PathBuf,
    planner: Options,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            archive: PathBuf::from("/srv/game-archive"),
            planner: Options::default(),
        }
    }
}

fn parse_filesystem(input: &str) -> std::result::Result<Filesystem, String> {
    match input.to_ascii_lowercase().as_str() {
        "fat32" => Ok(Filesystem::Fat32),
        "exfat" => Ok(Filesystem::Exfat),
        "ext4" => Ok(Filesystem::Ext4),
        _ => Err("supported filesystems: fat32, exfat, ext4".into()),
    }
}

fn parse_bytes(input: &str) -> std::result::Result<u64, String> {
    let split = input
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(input.len());
    let number = input[..split]
        .parse::<u64>()
        .map_err(|_| "capacity must be a nonnegative integer with an optional unit".to_owned())?;
    let unit = match input[split..].to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kib" => 1_u64 << 10,
        "m" | "mib" => 1 << 20,
        "g" | "gib" => 1 << 30,
        "t" | "tib" => 1 << 40,
        "kb" => 1_000,
        "mb" => 1_000_000,
        "gb" => 1_000_000_000,
        "tb" => 1_000_000_000_000,
        _ => {
            return Err(
                "units: B, K/M/G/T or KiB/MiB/GiB/TiB (binary), KB/MB/GB/TB (decimal)".into(),
            );
        }
    };
    number
        .checked_mul(unit)
        .ok_or_else(|| "capacity exceeds u64 bytes".into())
}

fn run(cli: Cli) -> Result<()> {
    let mut config: Config = if let Some(path) = cli.config {
        serde_json::from_slice(&fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)?
    } else {
        Config::default()
    };
    if let Some(archive) = cli.archive {
        config.archive = archive;
    }
    match cli.command {
        Command::Devices { snapshot } => {
            let inventory = if let Some(path) = snapshot {
                if !fs::symlink_metadata(&path)?.is_file() {
                    return Err("snapshot must be a regular file without symlinks".into());
                }
                let mut bytes = Vec::new();
                fs::File::open(&path)?
                    .take(usb_device::MAX_SNAPSHOT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                usb_device::parse_snapshot(&bytes)?
            } else {
                usb_device::inventory()?
            };
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&inventory)?);
            } else {
                for device in &inventory.devices {
                    println!(
                        "{:?}: {} | {:?} | {:?}",
                        device.block.path,
                        if device.candidate_for_review {
                            "CANDIDATE FOR REVIEW"
                        } else {
                            "REJECTED"
                        },
                        device.filesystem,
                        device.rejections
                    );
                }
                println!(
                    "Read-only metadata snapshot. Port identity and filesystem health are unverified; mounting, writes and safe removal are not authorised."
                );
            }
        }
        Command::CopyBatch {
            destinations,
            workers,
            filesystem,
            capacity_limit,
            budget,
        } => {
            budget.apply(&mut config.planner);
            let options = usb_transfer::TransferOptions {
                filesystem,
                planner: config.planner,
                capacity_limit,
            };
            let jobs: Vec<_> = destinations
                .into_iter()
                .enumerate()
                .map(|(index, destination)| usb_transfer::scheduler::Job {
                    id: format!("port-{}", index + 1),
                    archive: config.archive.clone(),
                    destination,
                    options: options.clone(),
                    cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                })
                .collect();
            let signals: Vec<_> = jobs.iter().map(|j| j.cancel.clone()).collect();
            ctrlc::set_handler(move || {
                for flag in &signals {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })?;
            let mut progress = std::collections::BTreeMap::<String, (u64, u64)>::new();
            let report = usb_transfer::scheduler::run_batch(&jobs, workers.into(), |event| {
                use usb_transfer::{Event, scheduler::JobEvent};
                if let JobEvent::Transfer { job_id, transfer } = event {
                    let counters = progress.entry(job_id.clone()).or_default();
                    if let Event::Prepared { payload_bytes, .. } = transfer {
                        counters.1 = *payload_bytes;
                    }
                    if let Event::CopyProgress { total_bytes, .. } = transfer {
                        if total_bytes.saturating_sub(counters.0) < 1024 * 1024
                            && *total_bytes != counters.1
                        {
                            return;
                        }
                        counters.0 = *total_bytes;
                    }
                }
                if cli.json {
                    if let Ok(json) = serde_json::to_string(event) {
                        eprintln!("{json}");
                    }
                } else {
                    eprintln!("{event:?}");
                }
            })?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Directory batch: {:?} | Workers: {} | Elapsed: {} ms | Confirmed payload: {} bytes",
                    report.status,
                    report.workers_started,
                    report.elapsed_millis,
                    report.confirmed_payload_bytes
                );
                for job in &report.jobs {
                    println!("{}: {:?} ({} ms)", job.id, job.status, job.elapsed_millis);
                    if let Some(failure) = &job.failure {
                        println!("Failure: {}", failure.message.escape_debug());
                    }
                    if let Some(transfer) = &job.report {
                        if let Some(failure) = &transfer.failure {
                            println!("Failure: {}", failure.message.escape_debug());
                        }
                        if let Some(id) = &transfer.unconfirmed_game {
                            println!("Unconfirmed: {id}; retain this destination for inspection.");
                        }
                    }
                }
                println!("Device mounting and safe removal are not managed by this command.");
            }
            if report.status != usb_transfer::Status::Completed {
                return Err(format!("directory batch {:?}", report.status).into());
            }
        }
        Command::InspectTransfer { destination } => {
            let inspection = usb_transfer::inspect_transfer(&destination)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&inspection)?);
            } else {
                println!(
                    "Recorded terminal status: {:?}\nRecorded completions: {:?}\nActive game: {:?}\nPublication uncertain: {}",
                    inspection.terminal_status,
                    inspection.recorded_completed,
                    inspection.active_game,
                    inspection.publication_uncertain
                );
                println!(
                    "Historical records only; content and ownership are unverified. Retain this tree. Retry with copy-to-directory into a fresh destination. Device safe removal is not managed."
                );
            }
        }
        Command::Catalogue { command } => {
            let root = match command {
                CatalogueCommand::Scan { path: Some(path) } => path,
                _ => config.archive,
            };
            let catalogue = catalogue::scan(&root)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&catalogue)?);
            } else {
                println!(
                    "Archive: {:?}\nPlatforms: {} | Collections: {} | Games: {}",
                    catalogue.root,
                    catalogue.platforms.len(),
                    catalogue.collections.len(),
                    catalogue.games.len()
                );
                for game in &catalogue.games {
                    let m = &game.metadata;
                    println!(
                        "{} | {:?} | {:?} / {:?} | {} bytes | {} files | approved: {} | source: {:?}",
                        m.id,
                        m.name,
                        catalogue.platforms[&m.platform].name,
                        catalogue.collections[&m.collection].name,
                        game.bytes,
                        game.files.len(),
                        m.redistribution_approved,
                        game.source
                    );
                }
            }
        }
        Command::Plan {
            capacity,
            filesystem,
            budget,
        } => {
            budget.apply(&mut config.planner);
            let catalogue = catalogue::scan(&config.archive)?;
            let plan = planner::plan(&catalogue, capacity, filesystem, &config.planner)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
            } else {
                println!(
                    "Available: {} bytes\nSafety reserve: {} bytes\nSafe capacity: {} bytes",
                    plan.available_bytes, plan.reserved_bytes, plan.safe_capacity
                );
                for game in &plan.selected {
                    println!(
                        "SELECT {} {:?}: {} payload bytes, {} estimated bytes -> {}",
                        game.id,
                        game.name,
                        game.payload_bytes,
                        game.estimated_bytes,
                        game.destination
                    );
                }
                println!(
                    "Selected games: {}\nPayload: {} bytes\nTotal planned: {} bytes (including estimated overhead)\nRemaining estimated capacity: {} bytes ({} usable after reserve)",
                    plan.selected.len(),
                    plan.payload_bytes,
                    plan.planned_bytes,
                    plan.remaining_available_bytes,
                    plan.remaining_safe_bytes
                );
                for game in &plan.skipped {
                    println!("SKIP {} {:?}: {:?}", game.id, game.name, game.reason);
                }
                println!(
                    "Skipped games: {}\nEstimate only. No destination devices were accessed.",
                    plan.skipped.len()
                );
            }
        }
        Command::CopyToDirectory {
            destination,
            filesystem,
            capacity_limit,
            budget,
        } => {
            budget.apply(&mut config.planner);
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let signal_cancel = cancel.clone();
            ctrlc::set_handler(move || {
                signal_cancel.store(true, std::sync::atomic::Ordering::Relaxed)
            })?;
            let options = usb_transfer::TransferOptions {
                filesystem,
                planner: config.planner,
                capacity_limit,
            };
            let mut last_progress = 0;
            let mut total_payload = 0;
            let report = usb_transfer::copy_to_directory(
                &config.archive,
                &destination,
                &options,
                &cancel,
                |event| {
                    if let usb_transfer::Event::Prepared { payload_bytes, .. } = event {
                        total_payload = *payload_bytes;
                    }
                    if let usb_transfer::Event::CopyProgress { total_bytes, .. } = event {
                        if total_bytes.saturating_sub(last_progress) < 1024 * 1024
                            && *total_bytes != total_payload
                        {
                            return;
                        }
                        last_progress = *total_bytes;
                    }
                    if cli.json {
                        // Event serialization is infallible for these bounded Rust types.
                        if let Ok(json) = serde_json::to_string(event) {
                            eprintln!("{json}");
                        }
                    } else {
                        eprintln!("{event:?}");
                    }
                },
            )?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Directory copy: {:?}\nSelected games: {}\nCompleted whole games: {}",
                    report.status,
                    report.plan.selected.len(),
                    report.completed.len()
                );
                for game in &report.completed {
                    println!(
                        "COMPLETE {}: {} verified payload bytes",
                        game.id, game.payload_bytes
                    );
                }
                for game in &report.plan.skipped {
                    println!("SKIP {}: {:?}", game.id, game.reason);
                }
                if let Some(failure) = &report.failure {
                    println!("Failure: {}", failure.message.escape_debug());
                }
                if let Some(id) = &report.unconfirmed_game {
                    println!("Unconfirmed game: {id}; inspect USB/.incoming and USB/games.");
                }
                println!("Device mounting and safe removal are not managed by this command.");
            }
            if report.status != usb_transfer::Status::Completed {
                return Err(format!("directory copy {:?}", report.status).into());
            }
        }
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("usb: {}", error.to_string().escape_debug());
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_units_and_invalid_inputs() {
        assert_eq!(parse_bytes("16G"), Ok(16 << 30));
        assert_eq!(parse_bytes("64GB"), Ok(64_000_000_000));
        assert_eq!(parse_bytes("0"), Ok(0));
        for input in ["", "-1", "1.5G", "100%", "18446744073709551615T"] {
            assert!(parse_bytes(input).is_err());
        }
    }
    #[test]
    fn cli_schema_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
