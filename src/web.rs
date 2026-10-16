use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};
use chrono::Datelike;
use serde::Serialize;

use crate::stats::{
    AuthorStats, BusFactorEntry, CommitInfo, FileStats, RepoStats, report_filename,
};

const INDEX: &str = include_str!("web/index.html");
const STYLE: &str = include_str!("web/style.css");
const SCRIPT: &str = include_str!("web/app.js");

const TOP_N: usize = 10;
const MONTH_SECS: i64 = 30 * 24 * 3600;

/// What the HTML report actually renders: aggregates and small top-N lists, never the
/// full per-commit / per-file data (unbounded, and useless once summarized into a chart).
#[derive(Serialize)]
struct WebData<'a> {
    from_ref: String,
    to_ref: String,
    total_commits: u64,
    total_additions: u64,
    total_deletions: u64,
    total_files_changed: u64,
    commits_by_hour: &'a [u64; 24],
    commits_by_weekday: &'a [u64; 7],
    commit_size_distribution: &'a [u64; 5],
    cohesion_score: u8,
    authors: &'a HashMap<String, AuthorStats>,
    bus_factors: &'a [BusFactorEntry],
    daily_commit_counts: HashMap<String, u64>,
    yearly_commit_counts: Vec<(i32, u64)>,
    latest_commit: Option<LatestCommit>,
    top_files: Vec<&'a FileStats>,
    top_impact_commits: Vec<ImpactCommit<'a>>,
    decay_buckets: [u64; 5],
    decay_total: u64,
    decay_stale: u64,
}

#[derive(Serialize)]
struct LatestCommit {
    id: String,
    timestamp: i64,
}

/// A commit's chart-relevant fields only — a full `CommitInfo` carries a `files: Vec<String>`
/// that, for a merge commit touching tens of thousands of files, can dwarf everything else here.
#[derive(Serialize)]
struct ImpactCommit<'a> {
    id: &'a str,
    author_name: &'a str,
    timestamp: i64,
    message: &'a str,
    additions: u64,
    deletions: u64,
}

/// Shorten a ref for display if it looks like a full commit SHA (e.g. a `from` ref
/// piped in from `git rev-list`) — tags and branch names are left untouched.
fn short_ref(r: &str) -> String {
    if r.len() >= 20 && r.bytes().all(|b| b.is_ascii_hexdigit()) {
        r[..7].to_string()
    } else {
        r.to_string()
    }
}

fn build_web_data(stats: &RepoStats) -> WebData<'_> {
    // At real-world scale a handful of commits carry garbage author timestamps (epoch-zero,
    // or absurd future dates from a misconfigured clock). Left unfiltered, a single such
    // outlier blows out the yearly chart's axis and the header's lifespan range. Totals
    // (total_commits etc.) are unaffected — this only guards the date-bucketed charts.
    let now_wall = chrono::Utc::now().timestamp();
    let is_plausible = |ts: i64| ts > 0 && ts <= now_wall + 86_400;

    let mut daily_commit_counts: HashMap<String, u64> = HashMap::new();
    let mut yearly_commit_counts: HashMap<i32, u64> = HashMap::new();
    for c in stats.commits.iter().filter(|c| is_plausible(c.timestamp)) {
        if let Some(dt) = chrono::DateTime::from_timestamp(c.timestamp, 0) {
            *daily_commit_counts
                .entry(dt.format("%Y-%m-%d").to_string())
                .or_default() += 1;
            *yearly_commit_counts.entry(dt.year()).or_default() += 1;
        }
    }
    let mut yearly_commit_counts: Vec<(i32, u64)> = yearly_commit_counts.into_iter().collect();
    yearly_commit_counts.sort_by_key(|(year, _)| *year);

    // Commits are collected newest-first; pick the first one with a sane timestamp.
    let latest = stats.commits.iter().find(|c| is_plausible(c.timestamp));
    let latest_commit = latest.map(|c| LatestCommit {
        id: c.id.clone(),
        timestamp: c.timestamp,
    });
    let now = latest.map(|c| c.timestamp).unwrap_or(now_wall);

    let mut top_files: Vec<&FileStats> = stats.files.values().collect();
    top_files.sort_by_key(|f| std::cmp::Reverse(f.change_count));
    top_files.truncate(TOP_N);

    let mut top_impact_commits: Vec<&CommitInfo> =
        stats.commits.iter().filter(|c| !c.is_bot).collect();
    top_impact_commits.sort_by_key(|c| std::cmp::Reverse(c.additions + c.deletions));
    top_impact_commits.truncate(TOP_N);
    let top_impact_commits = top_impact_commits
        .into_iter()
        .map(|c| ImpactCommit {
            id: &c.id,
            author_name: &c.author_name,
            timestamp: c.timestamp,
            message: &c.message,
            additions: c.additions,
            deletions: c.deletions,
        })
        .collect();

    let mut decay_buckets = [0u64; 5];
    let mut decay_total = 0u64;
    for f in stats.files.values() {
        if f.last_modified_ts <= 0 {
            continue;
        }
        decay_total += 1;
        let age = now - f.last_modified_ts;
        let bucket = if age < MONTH_SECS {
            0
        } else if age < 3 * MONTH_SECS {
            1
        } else if age < 6 * MONTH_SECS {
            2
        } else if age < 12 * MONTH_SECS {
            3
        } else {
            4
        };
        decay_buckets[bucket] += 1;
    }
    let decay_stale = decay_buckets[3] + decay_buckets[4];

    WebData {
        from_ref: short_ref(&stats.from_ref),
        to_ref: short_ref(&stats.to_ref),
        total_commits: stats.total_commits,
        total_additions: stats.total_additions,
        total_deletions: stats.total_deletions,
        total_files_changed: stats.total_files_changed,
        commits_by_hour: &stats.commits_by_hour,
        commits_by_weekday: &stats.commits_by_weekday,
        commit_size_distribution: &stats.commit_size_distribution,
        cohesion_score: stats.cohesion_score,
        authors: &stats.authors,
        bus_factors: &stats.bus_factors,
        daily_commit_counts,
        yearly_commit_counts,
        latest_commit,
        top_files,
        top_impact_commits,
        decay_buckets,
        decay_total,
        decay_stale,
    }
}

pub fn export(
    stats: &RepoStats,
    project: &str,
    hidden_charts: &[String],
    affiliations: &HashMap<String, String>,
) -> Result<()> {
    let data_json =
        serde_json::to_string(&build_web_data(stats)).context("failed to serialize stats")?;
    let hidden_json =
        serde_json::to_string(hidden_charts).context("failed to serialize hidden charts")?;
    // Lowercased so lookups against (lowercased) author emails are case-insensitive.
    let affiliations: HashMap<String, &String> = affiliations
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect();
    let affiliations_json =
        serde_json::to_string(&affiliations).context("failed to serialize affiliations")?;

    let html = INDEX
        .replace("{{STYLE}}", STYLE)
        .replace("{{SCRIPT}}", SCRIPT)
        .replace("{{REPO_NAME}}", project)
        .replace("{{DATA}}", &data_json)
        .replace("{{HIDDEN}}", &hidden_json)
        .replace("{{AFFILIATIONS}}", &affiliations_json);

    let filename = report_filename(project, &stats.from_ref, &stats.to_ref, "html");
    fs::write(&filename, &html).context("failed to write HTML file")?;

    eprintln!("  ✓ Exported to {filename}");
    Ok(())
}
