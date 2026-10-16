mod cache;
mod collector;
mod combine;
mod config;
mod revspec;
mod stats;
mod web;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use config::Config;
use stats::{ProjectStats, RepoStats};

#[derive(Clone, Default, clap::ValueEnum)]
enum Output {
    #[default]
    Web,
    Json,
}

#[derive(Parser)]
#[command(
    name = "gitat",
    about = "Git statistics for the changes between two branches or releases"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Git statistics for the changes between two branches or releases
    Diff {
        /// Path to the git repository
        path: PathBuf,

        /// Starting ref (branch, tag, or commit) or date (`2024-01-01`, `6 months ago`, ...) — the baseline
        from: String,

        /// Ending ref (branch, tag, or commit) or date — what changed since `from`
        to: String,

        /// Branch whose history dates are looked up on
        #[arg(long, short, default_value = "HEAD")]
        branch: String,

        /// Output format. Json is handy for combining stats across multiple repos.
        #[arg(long, short, default_value = "web")]
        output: Output,

        /// Path to a TOML config file (project name override, charts to hide)
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Merge multiple gitat json exports into one umbrella report
    Combine {
        /// Json files previously written with --output json
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Output format
        #[arg(long, short, default_value = "web")]
        output: Output,

        /// Path to a TOML config file (project name override, charts to hide)
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn default_project_name(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string())
}

fn write_output(output: &Output, project: &str, stats: RepoStats, config: &Config) -> Result<()> {
    match output {
        Output::Web => web::export(&stats, project, &config.hide, &config.affiliations),
        Output::Json => {
            let export = ProjectStats {
                project: project.to_string(),
                stats,
            };
            let filename = stats::report_filename(
                &export.project,
                &export.stats.from_ref,
                &export.stats.to_ref,
                "json",
            );
            let file = std::fs::File::create(&filename)?;
            serde_json::to_writer_pretty(file, &export)?;
            eprintln!("  ✓ Exported to {filename}");
            Ok(())
        }
    }
}

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn load_config(config: &Option<PathBuf>) -> Result<Config> {
    match config {
        Some(path) => Config::load(path),
        None => Ok(Config::default()),
    }
}

fn run_diff(
    path: PathBuf,
    from: String,
    to: String,
    branch: String,
    output: Output,
    config: Option<PathBuf>,
) -> Result<()> {
    let config = load_config(&config)?;
    let start = Instant::now();

    let (from_id, to_id) = {
        let repo = gix::open(&path).context("failed to open repository")?;
        (
            revspec::resolve_from(&repo, &from, &branch)?,
            revspec::resolve_to(&repo, &to, &branch)?,
        )
    };

    let mut stats = if let Some(mut cached) = cache::load(&path, from_id, to_id) {
        eprintln!("  ✓ Loaded from cache");
        // The same commits may have been reached through different refs/dates.
        cached.from_ref = from.clone();
        cached.to_ref = to.clone();
        cached
    } else {
        let commit_count = Arc::new(AtomicU64::new(0));
        let done = Arc::new(AtomicBool::new(false));

        let count_ref = Arc::clone(&commit_count);
        let done_ref = Arc::clone(&done);
        let spinner_handle = std::thread::spawn(move || {
            let mut i = 0usize;
            let mut stderr = std::io::stderr();
            while !done_ref.load(Ordering::Relaxed) {
                let n = count_ref.load(Ordering::Relaxed);
                let _ = write!(
                    stderr,
                    "\r  {} Parsing commits... {n}",
                    SPINNER[i % SPINNER.len()]
                );
                let _ = stderr.flush();
                i += 1;
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
            let n = count_ref.load(Ordering::Relaxed);
            let _ = write!(stderr, "\r  ✓ Parsed {n} commits              \n");
            let _ = stderr.flush();
        });

        let count_ref = Arc::clone(&commit_count);
        let stats = collector::collect(&path, &from, &to, from_id, to_id, |n| {
            count_ref.store(n, Ordering::Relaxed);
        })?;
        done.store(true, Ordering::Relaxed);
        spinner_handle.join().unwrap();

        cache::save(&path, from_id, to_id, &stats);
        stats
    };

    stats.mark_bots(&config.bots);

    let elapsed = start.elapsed();
    eprintln!("  Ready in {elapsed:.2?}");

    let project = config
        .name
        .clone()
        .unwrap_or_else(|| default_project_name(&path));
    write_output(&output, &project, stats, &config)
}

fn run_combine(files: Vec<PathBuf>, output: Output, config: Option<PathBuf>) -> Result<()> {
    let config = load_config(&config)?;

    let mut stats = combine::merge(&files)?;
    stats.mark_bots(&config.bots);
    eprintln!("  ✓ Combined {} projects", files.len());

    // A custom name means the report shouldn't leak its constituent projects.
    let project = match &config.name {
        Some(name) => {
            stats.to_ref.clear();
            name.clone()
        }
        None => "Combined".to_string(),
    };
    write_output(&output, &project, stats, &config)
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Diff {
            path,
            from,
            to,
            branch,
            output,
            config,
        } => run_diff(path, from, to, branch, output, config),
        Command::Combine {
            files,
            output,
            config,
        } => run_combine(files, output, config),
    }
}
