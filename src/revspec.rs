use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use gix::ObjectId;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

fn peel_commit(repo: &gix::Repository, spec: &str) -> Result<ObjectId> {
    Ok(repo
        .rev_parse_single(spec)
        .with_context(|| format!("failed to resolve '{spec}'"))?
        .object()
        .with_context(|| format!("failed to load object for '{spec}'"))?
        .peel_to_commit()
        .with_context(|| format!("'{spec}' does not resolve to a commit"))?
        .id)
}

/// `spec` as a date (`2024-01-01`, `2024-01-01 12:00:00 +0200`, `2 weeks ago`, ...), unless
/// it is a ref/rev, which always wins.
fn as_date(repo: &gix::Repository, spec: &str) -> Option<i64> {
    // A bare number would parse as seconds since the epoch, which is never what someone typing
    // `2024` or a mistyped short hash means.
    if repo.rev_parse_single(spec).is_ok() || spec.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    gix::date::parse(spec, Some(SystemTime::now()))
        .ok()
        .map(|t| t.seconds)
}

/// The first commit on `branch`'s first-parent history committed at or after `since`, along
/// with its first parent. First-parent only: "the branch as of that date", not a side-branch
/// commit that only landed (via a merge) later.
fn first_commit_since(
    repo: &gix::Repository,
    branch: &str,
    since: i64,
) -> Result<Option<(ObjectId, Option<ObjectId>)>> {
    let tip = peel_commit(repo, branch)?;
    let mut candidate = None;
    for info in repo
        .rev_walk([tip])
        .first_parent_only()
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .context("failed to walk commits")?
    {
        let info = info.context("failed to read commit info")?;
        // `info.commit_time` isn't populated for first-parent walks, read it off the commit.
        let commit_time = info
            .object()
            .context("failed to read commit")?
            .time()
            .context("failed to parse commit time")?
            .seconds;
        if commit_time < since {
            return Ok(candidate.map(|id| (id, Some(info.id))));
        }
        candidate = Some(info.id);
    }
    Ok(candidate.map(|id| (id, None)))
}

/// The commit to exclude, with its history, from the range: `from` itself for a ref, or the
/// parent of the first commit on/after the date so that commit is still counted. `None` means
/// the date predates the branch's history, so nothing is excluded.
pub fn resolve_from(repo: &gix::Repository, from: &str, branch: &str) -> Result<Option<ObjectId>> {
    let Some(since) = as_date(repo, from) else {
        return peel_commit(repo, from).map(Some);
    };
    match first_commit_since(repo, branch, since)? {
        Some((_, parent)) => Ok(parent),
        None => bail!("no commit on '{branch}' at or after '{from}'"),
    }
}

/// The end of the range: `to` itself for a ref, or the first commit on/after the date. A date
/// past the branch's latest commit means the whole branch.
pub fn resolve_to(repo: &gix::Repository, to: &str, branch: &str) -> Result<ObjectId> {
    let Some(since) = as_date(repo, to) else {
        return peel_commit(repo, to);
    };
    match first_commit_since(repo, branch, since)? {
        Some((id, _)) => Ok(id),
        None => peel_commit(repo, branch),
    }
}
