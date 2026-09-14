use std::fs;
use std::io::Write;
use std::path::Path;

use crate::stats::RepoStats;

fn cache_path(repo_path: &Path, from: &str, to: &str) -> Option<std::path::PathBuf> {
    let canonical = repo_path.canonicalize().ok()?;
    let key = format!("{}\n{from}\n{to}", canonical.to_string_lossy());
    let hash = simple_hash(key.as_bytes());
    let dir = dirs::cache_dir()?.join("gitat");
    Some(dir.join(format!("{hash}.json")))
}

fn simple_hash(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn resolve_commit_id(repo: &gix::Repository, r: &str) -> Option<String> {
    Some(
        repo.rev_parse_single(r)
            .ok()?
            .object()
            .ok()?
            .peel_to_commit()
            .ok()?
            .id
            .to_string(),
    )
}

fn resolve_ids(repo_path: &Path, from: &str, to: &str) -> Option<(String, String)> {
    let repo = gix::open(repo_path).ok()?;
    let from_id = resolve_commit_id(&repo, from)?;
    let to_id = resolve_commit_id(&repo, to)?;
    Some((from_id, to_id))
}

pub fn load(repo_path: &Path, from: &str, to: &str) -> Option<RepoStats> {
    let path = cache_path(repo_path, from, to)?;
    let data = fs::read_to_string(&path).ok()?;
    let stats: RepoStats = serde_json::from_str(&data).ok()?;

    let (from_id, to_id) = resolve_ids(repo_path, from, to)?;
    if stats.from_commit_id != from_id || stats.to_commit_id != to_id {
        return None;
    }

    Some(stats)
}

pub fn save(repo_path: &Path, from: &str, to: &str, stats: &RepoStats) {
    let Some(path) = cache_path(repo_path, from, to) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(json) = serde_json::to_string(stats) else {
        return;
    };
    let Ok(mut file) = fs::File::create(&path) else {
        return;
    };
    let _ = file.write_all(json.as_bytes());
}
