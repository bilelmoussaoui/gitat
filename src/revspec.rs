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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{ALICE, TestRepo};

    /// main: c1 (Jan 10) - c2 (Feb 10) - c3 (Mar 10) - c4 (Apr 10), all at 12:00 UTC.
    fn linear() -> (TestRepo, [ObjectId; 4]) {
        let repo = TestRepo::new();
        let ids = ["2024-01-10", "2024-02-10", "2024-03-10", "2024-04-10"]
            .map(|d| repo.commit(&[("f", d)], d, ALICE, &format!("{d}T12:00:00Z")));
        (repo, ids)
    }

    #[test]
    fn refs_resolve_as_is() {
        let (repo, [_, c2, c3, c4]) = linear();
        repo.tag("v1");
        let r = repo.open();
        assert_eq!(resolve_to(&r, "v1", "HEAD").unwrap(), c4);
        assert_eq!(resolve_to(&r, "HEAD~1", "HEAD").unwrap(), c3);
        assert_eq!(resolve_from(&r, "HEAD~2", "HEAD").unwrap(), Some(c2));
    }

    #[test]
    fn a_ref_wins_over_a_date() {
        let (repo, [_, _, c3, _]) = linear();
        repo.checkout(&[&c3.to_string()]);
        repo.tag("2024-01-01");
        repo.checkout(&["main"]);
        let r = repo.open();
        assert_eq!(resolve_to(&r, "2024-01-01", "main").unwrap(), c3);
    }

    #[test]
    fn to_date_picks_first_commit_at_or_after() {
        let (repo, [_, c2, _, c4]) = linear();
        let r = repo.open();
        assert_eq!(resolve_to(&r, "2024-02-01", "HEAD").unwrap(), c2);
        assert_eq!(
            resolve_to(&r, "2024-02-10 12:00:00 +0000", "HEAD").unwrap(),
            c2
        );
        // Past the latest commit: the whole branch.
        assert_eq!(resolve_to(&r, "2030-01-01", "HEAD").unwrap(), c4);
        assert_eq!(resolve_to(&r, "1 week ago", "HEAD").unwrap(), c4);
    }

    #[test]
    fn from_date_keeps_the_commit_on_that_date() {
        let (repo, [c1, _, _, _]) = linear();
        let r = repo.open();
        // c2 is the first commit on/after Feb 1st, so its parent is what gets excluded.
        assert_eq!(resolve_from(&r, "2024-02-01", "HEAD").unwrap(), Some(c1));
        // Before all history: nothing excluded.
        assert_eq!(resolve_from(&r, "2023-01-01", "HEAD").unwrap(), None);
    }

    #[test]
    fn from_date_after_history_is_an_error() {
        let (repo, _) = linear();
        let err = resolve_from(&repo.open(), "2030-01-01", "HEAD").unwrap_err();
        assert!(err.to_string().contains("at or after"), "{err}");
    }

    #[test]
    fn bare_numbers_and_garbage_are_not_dates() {
        let (repo, _) = linear();
        let r = repo.open();
        for spec in ["2024", "nonsense"] {
            let err = resolve_to(&r, spec, "HEAD").unwrap_err();
            assert!(
                err.to_string().contains("failed to resolve"),
                "{spec}: {err}"
            );
        }
    }

    #[test]
    fn dates_follow_first_parent_history() {
        let repo = TestRepo::new();
        let c1 = repo.commit(&[("f", "1")], "c1", ALICE, "2024-01-10T12:00:00Z");
        repo.checkout(&["-b", "feature"]);
        let side = repo.commit(&[("g", "1")], "side", ALICE, "2024-02-20T12:00:00Z");
        repo.checkout(&["main"]);
        let c2 = repo.commit(&[("f", "2")], "c2", ALICE, "2024-03-10T12:00:00Z");
        repo.merge("feature", "2024-03-15T12:00:00Z");
        let r = repo.open();
        // The side-branch commit (Feb 20) only reached main on Mar 15, so it isn't picked.
        assert_eq!(resolve_to(&r, "2024-02-15", "main").unwrap(), c2);
        assert_eq!(resolve_from(&r, "2024-02-15", "main").unwrap(), Some(c1));
        // But it is when walking the feature branch itself.
        assert_eq!(resolve_to(&r, "2024-02-15", "feature").unwrap(), side);
    }
}
