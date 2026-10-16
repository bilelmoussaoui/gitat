use std::fs;
use std::io::Write;
use std::path::Path;

use gix::ObjectId;

use crate::stats::RepoStats;

fn cache_path(
    repo_path: &Path,
    from_id: Option<ObjectId>,
    to_id: ObjectId,
) -> Option<std::path::PathBuf> {
    let canonical = repo_path.canonicalize().ok()?;
    let from_id = from_id.map(|id| id.to_string()).unwrap_or_default();
    let key = format!("{}\n{from_id}\n{to_id}", canonical.to_string_lossy());
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

/// Keyed by the resolved commits rather than the refs/dates given on the command line, so
/// e.g. `main` moving on, or two dates landing on the same commits, are handled for free.
pub fn load(repo_path: &Path, from_id: Option<ObjectId>, to_id: ObjectId) -> Option<RepoStats> {
    let path = cache_path(repo_path, from_id, to_id)?;
    let data = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save(repo_path: &Path, from_id: Option<ObjectId>, to_id: ObjectId, stats: &RepoStats) {
    let Some(path) = cache_path(repo_path, from_id, to_id) else {
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
