use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result};
use gix::ObjectId;
use gix::object::tree::diff::Action;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};
use regex::Regex;

use crate::stats::{
    AuthorStats, BusFactorEntry, CommitInfo, CommitRef, FileStats, RepoStats, TagInfo,
};

// Require an explicit #123 / !123 marker or a trailing /123 from an issue URL, not just
// any word after "fix(es)" — plain prose like "this fixes a race" would otherwise match "a".
static FIXES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bfix(?:e[sd])?\s+(?:(?:#|!)|\S*/)(\d+)\b").unwrap());
static CLOSES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bclose[sd]?\s+(?:(?:#|!)|\S*/)(\d+)\b").unwrap());
static RESOLVES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bresolve[sd]?\s+(?:(?:#|!)|\S*/)(\d+)\b").unwrap());

static AI_COAUTHOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)^co-authored-by:\s*(.+?)\s*<").unwrap());

const AI_NAMES: &[(&str, &str)] = &[
    ("claude", "Claude"),
    ("copilot", "Copilot"),
    ("cursor", "Cursor"),
    ("gemini", "Gemini"),
    ("gpt", "GPT"),
    ("openai", "OpenAI"),
    ("anthropic", "Anthropic"),
    ("codeium", "Codeium"),
    ("cody", "Cody"),
    ("devin", "Devin"),
    ("codex", "Codex"),
    ("amazon q", "Amazon Q"),
];

fn detect_ai_model(message: &str) -> Option<String> {
    for cap in AI_COAUTHOR_RE.captures_iter(message) {
        let name = &cap[1];
        let lower = name.to_lowercase();
        for &(pattern, label) in AI_NAMES {
            if lower.contains(pattern) {
                return Some(label.to_string());
            }
        }
    }

    static AI_GENERATED_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(generated|assisted|created)\s+(by|with|using)\s+(\S+)").unwrap()
    });
    for cap in AI_GENERATED_RE.captures_iter(message) {
        let tool = &cap[3];
        let lower = tool.to_lowercase();
        for &(pattern, label) in AI_NAMES {
            if lower.contains(pattern) {
                return Some(label.to_string());
            }
        }
    }

    None
}

const BOT_PATTERNS: &[&str] = &[
    "[bot]",
    "dependabot",
    "renovate",
    "greenkeeper",
    "snyk-bot",
    "github-actions",
    "gitlab-bot",
    "mergify",
    "codecov",
    "semantic-release-bot",
    "marge-bot",
];

fn is_bot(name: &str, email: &str) -> bool {
    let name_lower = name.to_lowercase();
    let email_lower = email.to_lowercase();
    BOT_PATTERNS
        .iter()
        .any(|p| name_lower.contains(p) || email_lower.contains(p))
}

struct CommitMeta {
    id: String,
    name: String,
    email: String,
    timestamp: i64,
    message: String,
    ai_model: Option<String>,
    is_bot: bool,
    refs: Vec<CommitRef>,
    tree_id: ObjectId,
    parent_tree_id: Option<ObjectId>,
}

struct DiffResult {
    additions: u64,
    deletions: u64,
    files_changed: u64,
    file_diffs: Vec<(String, u64, u64)>,
}

fn extract_commit_meta(repo: &gix::Repository, id: ObjectId) -> Result<CommitMeta> {
    let commit = repo
        .find_object(id)
        .context("failed to read commit object")?
        .try_into_commit()
        .context("object is not a commit")?;

    let author = commit.author().context("failed to read author")?;
    let name = author.name.to_string();
    let email = author.email.to_string();
    let timestamp = author
        .time()
        .context("failed to parse author time")?
        .seconds;

    let message = commit.message_raw_sloppy().to_string();
    let ai_model = detect_ai_model(&message);
    let bot = is_bot(&name, &email);

    let mut refs = Vec::new();
    for cap in FIXES_RE.captures_iter(&message) {
        refs.push(CommitRef::Fixes(cap[1].to_string()));
    }
    for cap in CLOSES_RE.captures_iter(&message) {
        refs.push(CommitRef::Closes(cap[1].to_string()));
    }
    for cap in RESOLVES_RE.captures_iter(&message) {
        refs.push(CommitRef::Resolves(cap[1].to_string()));
    }

    let tree_id = commit.tree_id().context("failed to get tree id")?.detach();
    let parent_tree_id = commit
        .parent_ids()
        .next()
        .and_then(|pid| pid.object().ok())
        .and_then(|obj| obj.try_into_commit().ok())
        .and_then(|c| c.tree_id().ok())
        .map(|id| id.detach());

    Ok(CommitMeta {
        id: id.to_string(),
        name,
        email,
        timestamp,
        message,
        ai_model,
        is_bot: bot,
        refs,
        tree_id,
        parent_tree_id,
    })
}

/// Per-thread accumulator for phase 4 (`fold`), combined pairwise across threads (`reduce`).
#[derive(Default)]
struct MergedChunk {
    total_commits: u64,
    total_additions: u64,
    total_deletions: u64,
    total_files_changed: u64,
    commits_by_hour: [u64; 24],
    commits_by_weekday: [u64; 7],
    commit_size_distribution: [u64; 5],
    authors: HashMap<String, AuthorStats>,
    files: HashMap<String, FileStats>,
    dir_author_changes: HashMap<String, HashMap<String, u64>>,
    commits: Vec<CommitInfo>,
}

fn fold_commit(mut acc: MergedChunk, (meta, diff): (CommitMeta, DiffResult)) -> MergedChunk {
    if let Some(dt) = chrono::DateTime::from_timestamp(meta.timestamp, 0) {
        use chrono::{Datelike, Timelike};
        let hour = dt.hour() as usize;
        let weekday = dt.weekday().num_days_from_sunday() as usize;
        acc.commits_by_hour[hour] += 1;
        acc.commits_by_weekday[weekday] += 1;
    }

    acc.total_commits += 1;
    acc.total_additions += diff.additions;
    acc.total_deletions += diff.deletions;
    acc.total_files_changed += diff.files_changed;

    let author_entry = acc
        .authors
        .entry(meta.email.clone())
        .or_insert_with(|| AuthorStats {
            name: meta.name.clone(),
            email: meta.email.clone(),
            is_bot: meta.is_bot,
            first_commit_ts: meta.timestamp,
            last_commit_ts: meta.timestamp,
            ..Default::default()
        });

    author_entry.commits += 1;
    author_entry.additions += diff.additions;
    author_entry.deletions += diff.deletions;
    author_entry.files_changed += diff.files_changed;
    if meta.ai_model.is_some() {
        author_entry.ai_assisted += 1;
    }

    if meta.timestamp < author_entry.first_commit_ts {
        author_entry.first_commit_ts = meta.timestamp;
    }
    if meta.timestamp > author_entry.last_commit_ts {
        author_entry.last_commit_ts = meta.timestamp;
    }

    for r in &meta.refs {
        match r {
            CommitRef::Fixes(s) => author_entry.fixes.push(s.clone()),
            CommitRef::Closes(s) => author_entry.closes.push(s.clone()),
            CommitRef::Resolves(_) => {}
        }
    }

    let commit_files: Vec<String> = diff.file_diffs.iter().map(|(p, _, _)| p.clone()).collect();

    for (path, adds, dels) in &diff.file_diffs {
        let entry = acc.files.entry(path.clone()).or_insert_with(|| FileStats {
            path: path.clone(),
            ..Default::default()
        });
        entry.additions += adds;
        entry.deletions += dels;
        entry.change_count += 1;
        if meta.timestamp > entry.last_modified_ts {
            entry.last_modified_ts = meta.timestamp;
        }

        let lines_changed = adds + dels;
        if lines_changed > 0 {
            *acc.dir_author_changes
                .entry(extract_directory(path))
                .or_default()
                .entry(meta.email.clone())
                .or_default() += lines_changed;
        }
    }

    let first_line = meta.message.lines().next().unwrap_or("").to_string();
    acc.commits.push(CommitInfo {
        id: meta.id,
        author_name: meta.name,
        author_email: meta.email,
        timestamp: meta.timestamp,
        message: first_line,
        additions: diff.additions,
        deletions: diff.deletions,
        files_changed: diff.files_changed,
        refs: meta.refs,
        ai_model: meta.ai_model,
        is_bot: meta.is_bot,
        files: commit_files,
    });

    let total_churn = diff.additions + diff.deletions;
    let bucket = match total_churn {
        0..10 => 0,
        10..100 => 1,
        100..500 => 2,
        500..1000 => 3,
        _ => 4,
    };
    acc.commit_size_distribution[bucket] += 1;

    acc
}

fn merge_author_stats(entry: &mut AuthorStats, other: AuthorStats) {
    entry.commits += other.commits;
    entry.additions += other.additions;
    entry.deletions += other.deletions;
    entry.files_changed += other.files_changed;
    entry.ai_assisted += other.ai_assisted;
    entry.fixes.extend(other.fixes);
    entry.closes.extend(other.closes);
    entry.first_commit_ts = entry.first_commit_ts.min(other.first_commit_ts);
    entry.last_commit_ts = entry.last_commit_ts.max(other.last_commit_ts);
}

fn merge_file_stats(entry: &mut FileStats, other: FileStats) {
    entry.additions += other.additions;
    entry.deletions += other.deletions;
    entry.change_count += other.change_count;
    entry.last_modified_ts = entry.last_modified_ts.max(other.last_modified_ts);
}

fn merge_chunks(mut a: MergedChunk, b: MergedChunk) -> MergedChunk {
    a.total_commits += b.total_commits;
    a.total_additions += b.total_additions;
    a.total_deletions += b.total_deletions;
    a.total_files_changed += b.total_files_changed;
    for i in 0..24 {
        a.commits_by_hour[i] += b.commits_by_hour[i];
    }
    for i in 0..7 {
        a.commits_by_weekday[i] += b.commits_by_weekday[i];
    }
    for i in 0..5 {
        a.commit_size_distribution[i] += b.commit_size_distribution[i];
    }

    for (email, author) in b.authors {
        match a.authors.entry(email) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                merge_author_stats(o.get_mut(), author)
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(author);
            }
        }
    }

    for (path, file) in b.files {
        match a.files.entry(path) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                merge_file_stats(o.get_mut(), file)
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(file);
            }
        }
    }

    for (dir, counts) in b.dir_author_changes {
        let entry = a.dir_author_changes.entry(dir).or_default();
        for (email, n) in counts {
            *entry.entry(email).or_default() += n;
        }
    }

    a.commits.extend(b.commits);
    a
}

pub fn collect(
    path: &Path,
    from: &str,
    to: &str,
    on_progress: impl Fn(u64) + Send + Sync,
) -> Result<RepoStats> {
    let mut repo = gix::open(path).context("failed to open repository")?;
    repo.object_cache_size(64 * 1024 * 1024);

    let from_id = repo
        .rev_parse_single(from)
        .with_context(|| format!("failed to resolve '{from}'"))?
        .object()
        .with_context(|| format!("failed to load object for '{from}'"))?
        .peel_to_commit()
        .with_context(|| format!("'{from}' does not resolve to a commit"))?
        .id;
    let to_commit = repo
        .rev_parse_single(to)
        .with_context(|| format!("failed to resolve '{to}'"))?
        .object()
        .with_context(|| format!("failed to load object for '{to}'"))?
        .peel_to_commit()
        .with_context(|| format!("'{to}' does not resolve to a commit"))?;
    let to_id = to_commit.id;
    let to_tree_id = to_commit
        .tree_id()
        .context("failed to get 'to' tree id")?
        .detach();
    drop(to_commit);

    let sync_repo = Arc::new(repo.into_sync());

    // Phase 1: sequential walk, just to collect commit ids in traversal order (cheap:
    // no object reads beyond what the walk itself needs).
    let commit_ids: Vec<ObjectId> = {
        let repo = sync_repo.to_thread_local();
        let mut ids = Vec::new();
        for info in repo
            .rev_walk([to_id])
            .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
            .with_hidden([from_id])
            .all()
            .context("failed to walk commits")?
        {
            ids.push(info.context("failed to read commit info")?.id().detach());
        }
        ids
    };

    // Phase 2: parallel commit metadata extraction (author, message, refs, tree ids)
    let metas: Vec<CommitMeta> = commit_ids
        .par_iter()
        .map_init(
            || {
                let mut repo = sync_repo.to_thread_local();
                repo.object_cache_size(64 * 1024 * 1024);
                repo
            },
            |repo, id| extract_commit_meta(repo, *id),
        )
        .collect::<Result<_>>()?;

    // Phase 3: parallel diff computation
    let progress = AtomicU64::new(0);

    let diff_results: Vec<DiffResult> = metas
        .par_iter()
        .map_init(
            || {
                let mut repo = sync_repo.to_thread_local();
                repo.object_cache_size(64 * 1024 * 1024);
                let diff_cache = repo.diff_resource_cache_for_tree_diff().unwrap();
                (repo, diff_cache)
            },
            |(repo, diff_cache), meta| {
                let result = compute_diff(repo, diff_cache, meta);
                let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
                on_progress(n);
                result
            },
        )
        .collect();

    // Phase 4: parallel fold + reduce merge of metas/diffs into stats
    let merged = metas
        .into_par_iter()
        .zip(diff_results.into_par_iter())
        .fold(MergedChunk::default, fold_commit)
        .reduce(MergedChunk::default, merge_chunks);

    let mut stats = RepoStats {
        from_ref: from.to_string(),
        to_ref: to.to_string(),
        from_commit_id: from_id.to_string(),
        to_commit_id: to_id.to_string(),
        total_commits: merged.total_commits,
        total_additions: merged.total_additions,
        total_deletions: merged.total_deletions,
        total_files_changed: merged.total_files_changed,
        commits_by_hour: merged.commits_by_hour,
        commits_by_weekday: merged.commits_by_weekday,
        commit_size_distribution: merged.commit_size_distribution,
        authors: merged.authors,
        files: merged.files,
        commits: merged.commits,
        ..Default::default()
    };
    // fold/reduce doesn't preserve the walk's newest-first order (web.rs relies on it).
    stats
        .commits
        .sort_by_key(|c| std::cmp::Reverse(c.timestamp));

    let dir_author_changes = merged.dir_author_changes;

    // Compute cohesion score
    let total = stats.total_commits.max(1) as f64;
    let healthy = (stats.commit_size_distribution[0]
        + stats.commit_size_distribution[1]
        + stats.commit_size_distribution[2]) as f64;
    stats.cohesion_score = (healthy / total * 100.0).round() as u8;

    // Filter to only files present at `to`
    let repo = sync_repo.to_thread_local();
    let to_tree = repo
        .find_object(to_tree_id)
        .context("failed to find 'to' tree")?
        .try_into_tree()
        .context("'to' object is not a tree")?;
    let to_files: HashSet<String> = to_tree
        .traverse()
        .breadthfirst
        .files()
        .context("failed to traverse 'to' tree")?
        .into_iter()
        .filter(|e| !e.mode.is_tree())
        .map(|e| e.filepath.to_string())
        .collect();
    stats.files.retain(|path, _| to_files.contains(path));

    // Build set of directories that still have files at `to`
    let to_dirs: HashSet<String> = to_files.iter().map(|p| extract_directory(p)).collect();

    // Compute bus factors per directory (only for directories still present at `to`)
    for (directory, author_map) in &dir_author_changes {
        if !to_dirs.contains(directory) {
            continue;
        }
        let total_dir: u64 = author_map.values().sum();
        if total_dir == 0 {
            continue;
        }

        let mut sorted: Vec<_> = author_map.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        let threshold = (total_dir as f64 * 0.80) as u64;
        let mut cumulative = 0u64;
        let mut bus_factor = 0u8;
        let mut top_authors = Vec::new();

        for (email, changes) in &sorted {
            cumulative += **changes;
            bus_factor += 1;
            let pct = (**changes as f64 / total_dir as f64 * 100.0 * 10.0).round() / 10.0;
            let name = stats
                .authors
                .get(*email)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| (*email).clone());
            top_authors.push((name, pct));
            if cumulative >= threshold {
                break;
            }
        }

        stats.bus_factors.push(BusFactorEntry {
            directory: directory.clone(),
            bus_factor,
            top_authors,
            total_changes: total_dir,
        });
    }
    stats.bus_factors.sort_by_key(|a| a.bus_factor);

    // Collect tags
    if let Ok(tag_refs) = repo.references()
        && let Ok(tags) = tag_refs.tags()
    {
        for tag_result in tags {
            let Ok(mut tag_ref) = tag_result else {
                continue;
            };
            let name = tag_ref.name().shorten().to_string();
            let Ok(peeled_id) = tag_ref.peel_to_id() else {
                continue;
            };
            let commit_id = peeled_id.to_string();

            let direct_id = tag_ref.id();
            let is_annotated = *direct_id != *peeled_id;

            let timestamp = if is_annotated {
                repo.find_object(direct_id.detach())
                    .ok()
                    .and_then(|obj| obj.try_into_tag().ok())
                    .and_then(|tag| {
                        tag.tagger()
                            .ok()
                            .flatten()
                            .and_then(|sig| sig.time().ok())
                            .map(|t| t.seconds)
                    })
                    .unwrap_or(0)
            } else {
                repo.find_object(peeled_id.detach())
                    .ok()
                    .and_then(|obj| obj.try_into_commit().ok())
                    .and_then(|c| {
                        c.author()
                            .ok()
                            .and_then(|a| a.time().ok())
                            .map(|t| t.seconds)
                    })
                    .unwrap_or(0)
            };

            stats.tags.push(TagInfo {
                name,
                commit_id,
                timestamp,
                is_annotated,
            });
        }
    }
    stats.tags.sort_by_key(|t| std::cmp::Reverse(t.timestamp));

    Ok(stats)
}

fn extract_directory(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 1 {
        return "(root)".to_string();
    }
    if parts[0] == "src" && parts.len() > 2 {
        format!("{}/{}", parts[0], parts[1])
    } else {
        parts[0].to_string()
    }
}

fn compute_diff(
    repo: &gix::Repository,
    diff_cache: &mut gix::diff::blob::Platform,
    meta: &CommitMeta,
) -> DiffResult {
    let mut result = DiffResult {
        additions: 0,
        deletions: 0,
        files_changed: 0,
        file_diffs: Vec::new(),
    };

    let Some(parent_tree_id) = meta.parent_tree_id else {
        return result;
    };

    let Ok(parent_obj) = repo.find_object(parent_tree_id) else {
        return result;
    };
    let Ok(parent_tree) = parent_obj.try_into_tree() else {
        return result;
    };

    let Ok(tree_obj) = repo.find_object(meta.tree_id) else {
        return result;
    };
    let Ok(tree) = tree_obj.try_into_tree() else {
        return result;
    };

    let Ok(mut changes) = parent_tree.changes() else {
        return result;
    };

    let file_diffs = &mut result.file_diffs;
    let additions = &mut result.additions;
    let deletions = &mut result.deletions;
    let files_changed = &mut result.files_changed;

    let _ = changes.for_each_to_obtain_tree(&tree, |change| {
        let entry_mode = match &change {
            gix::object::tree::diff::Change::Addition { entry_mode, .. }
            | gix::object::tree::diff::Change::Deletion { entry_mode, .. }
            | gix::object::tree::diff::Change::Modification { entry_mode, .. }
            | gix::object::tree::diff::Change::Rewrite { entry_mode, .. } => *entry_mode,
        };

        if entry_mode.is_tree() {
            return Ok::<_, std::convert::Infallible>(Action::Continue(()));
        }

        let file_path = match &change {
            gix::object::tree::diff::Change::Addition { location, .. }
            | gix::object::tree::diff::Change::Deletion { location, .. }
            | gix::object::tree::diff::Change::Modification { location, .. } => {
                location.to_string()
            }
            gix::object::tree::diff::Change::Rewrite { location, .. } => location.to_string(),
        };

        let mut file_adds: u64 = 0;
        let mut file_dels: u64 = 0;

        if let Ok(mut platform) = change.diff(diff_cache)
            && let Ok(Some(counts)) = platform.line_counts()
        {
            file_adds = counts.insertions as u64;
            file_dels = counts.removals as u64;
            *additions += file_adds;
            *deletions += file_dels;
            *files_changed += 1;
        }

        if !file_path.is_empty() {
            file_diffs.push((file_path, file_adds, file_dels));
        }

        diff_cache.clear_resource_cache_keep_allocation();
        Ok::<_, std::convert::Infallible>(Action::Continue(()))
    });

    result
}
