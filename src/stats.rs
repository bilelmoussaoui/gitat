use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct RepoStats {
    pub from_ref: String,
    pub to_ref: String,
    pub from_commit_id: String,
    pub to_commit_id: String,
    pub total_commits: u64,
    pub total_additions: u64,
    pub total_deletions: u64,
    pub total_files_changed: u64,
    pub authors: HashMap<String, AuthorStats>,
    pub commits: Vec<CommitInfo>,
    pub files: HashMap<String, FileStats>,
    pub commits_by_hour: [u64; 24],
    pub commits_by_weekday: [u64; 7],
    #[serde(default)]
    pub bus_factors: Vec<BusFactorEntry>,
    #[serde(default)]
    pub commit_size_distribution: [u64; 5],
    #[serde(default)]
    pub cohesion_score: u8,
    #[serde(default)]
    pub tags: Vec<TagInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub commit_id: String,
    pub timestamp: i64,
    pub is_annotated: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BusFactorEntry {
    pub directory: String,
    pub bus_factor: u8,
    pub top_authors: Vec<(String, f64)>,
    pub total_changes: u64,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct FileStats {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
    pub change_count: u64,
    #[serde(default)]
    pub last_modified_ts: i64,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct AuthorStats {
    pub name: String,
    pub email: String,
    pub commits: u64,
    pub additions: u64,
    pub deletions: u64,
    pub files_changed: u64,
    pub fixes: Vec<String>,
    pub closes: Vec<String>,
    pub ai_assisted: u64,
    pub is_bot: bool,
    pub first_commit_ts: i64,
    pub last_commit_ts: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitInfo {
    pub id: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub message: String,
    pub additions: u64,
    pub deletions: u64,
    pub files_changed: u64,
    pub refs: Vec<CommitRef>,
    pub ai_model: Option<String>,
    pub is_bot: bool,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CommitRef {
    Fixes(String),
    Closes(String),
    Resolves(String),
}

/// The shape written by `--output json` and read back by `gitat combine`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectStats {
    pub project: String,
    #[serde(flatten)]
    pub stats: RepoStats,
}

/// Collapse anything filename-unsafe (ref slashes, combined-report ", " separators, ...) into `-`.
fn sanitize_ref(r: &str) -> String {
    let mut out = String::with_capacity(r.len());
    for c in r.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Build a filename-safe report name, dropping empty parts (e.g. a combined report's `from_ref`).
pub fn report_filename(project: &str, from_ref: &str, to_ref: &str, extension: &str) -> String {
    let parts: Vec<String> = [project, from_ref, to_ref]
        .into_iter()
        .map(sanitize_ref)
        .filter(|s| !s.is_empty())
        .collect();
    format!("{}.{extension}", parts.join("-"))
}
