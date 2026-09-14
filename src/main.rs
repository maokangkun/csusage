//! csusage — token usage reports for Claude Science.
//!
//! Reads the local Claude Science metadata database (read-only) and prints
//! daily, monthly, or per-session token usage reports.

mod discovery;
mod loader;
mod report;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "csusage",
    version,
    about = "Token usage reports for Claude Science",
    long_about = "Reads the local Claude Science metadata database (read-only) and prints daily, monthly, or per-session token usage reports."
)]
struct Cli {
    #[command(subcommand)]
    report: Option<Report>,

    /// Path to the metadata database (overrides CLAUDE_SCIENCE_DB and discovery)
    #[arg(long, global = true)]
    db: Option<String>,

    /// First date to include, YYYY-MM-DD
    #[arg(long, global = true)]
    since: Option<String>,

    /// Last date to include, YYYY-MM-DD
    #[arg(long, global = true)]
    until: Option<String>,

    /// Timezone for date bucketing (IANA name, default UTC)
    #[arg(long, global = true)]
    timezone: Option<String>,

    /// Emit machine-readable JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand, Clone, Copy)]
enum Report {
    /// Per-day usage
    Daily,
    /// Per-month usage
    Monthly,
    /// Per-session usage
    Session,
}

fn main() {
    let cli = Cli::parse();
    let report = cli.report.unwrap_or(Report::Daily);
    if let Err(error) = run(cli, report) {
        eprintln!("csusage: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli, report: Report) -> Result<(), String> {
    let databases = discovery::database_paths(cli.db.as_deref())?;
    if databases.is_empty() {
        return Err(
            "no Claude Science database found; set CLAUDE_SCIENCE_DB or pass --db".to_string(),
        );
    }
    let mut frames = Vec::new();
    for path in &databases {
        match loader::read_frames(path) {
            Ok(mut frames_for_path) => frames.append(&mut frames_for_path),
            Err(error) => eprintln!("csusage: skipping {path:?}: {error}"),
        }
    }
    let timezone = report::parse_timezone(cli.timezone.as_deref())?;
    let rows = report::aggregate(
        &frames,
        report,
        cli.since.as_deref(),
        cli.until.as_deref(),
        &timezone,
    )?;
    if cli.json {
        report::print_json(&rows);
    } else {
        report::print_table(&rows, report);
    }
    Ok(())
}
