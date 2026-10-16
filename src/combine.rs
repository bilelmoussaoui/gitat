use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::stats::{AuthorStats, FileStats, ProjectStats, RepoStats};

/// Merge several `--output json` exports into one umbrella `RepoStats`.
pub fn merge(files: &[impl AsRef<Path>]) -> Result<RepoStats> {
    let mut out = RepoStats::default();
    let mut project_names = Vec::new();

    for path in files {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read '{}'", path.display()))?;
        let ProjectStats { project, stats } = serde_json::from_str(&text).with_context(|| {
            format!(
                "failed to parse '{}' as a gitat json export",
                path.display()
            )
        })?;

        project_names.push(project.clone());

        out.total_commits += stats.total_commits;
        out.total_additions += stats.total_additions;
        out.total_deletions += stats.total_deletions;
        out.total_files_changed += stats.total_files_changed;

        for (a, b) in out.commits_by_hour.iter_mut().zip(stats.commits_by_hour) {
            *a += b;
        }
        for (a, b) in out
            .commits_by_weekday
            .iter_mut()
            .zip(stats.commits_by_weekday)
        {
            *a += b;
        }
        for (a, b) in out
            .commit_size_distribution
            .iter_mut()
            .zip(stats.commit_size_distribution)
        {
            *a += b;
        }

        for (email, author) in stats.authors {
            let entry = out
                .authors
                .entry(email.clone())
                .or_insert_with(|| AuthorStats {
                    name: author.name.clone(),
                    email,
                    is_bot: author.is_bot,
                    first_commit_ts: author.first_commit_ts,
                    last_commit_ts: author.last_commit_ts,
                    ..Default::default()
                });
            entry.commits += author.commits;
            entry.additions += author.additions;
            entry.deletions += author.deletions;
            entry.files_changed += author.files_changed;
            entry.ai_assisted += author.ai_assisted;
            entry.is_bot |= author.is_bot;
            entry.fixes.extend(author.fixes);
            entry.closes.extend(author.closes);
            entry.first_commit_ts = entry.first_commit_ts.min(author.first_commit_ts);
            entry.last_commit_ts = entry.last_commit_ts.max(author.last_commit_ts);
        }

        out.commits.extend(stats.commits);

        for (path, file_stats) in stats.files {
            let key = format!("{project}/{path}");
            out.files.insert(
                key.clone(),
                FileStats {
                    path: key,
                    ..file_stats
                },
            );
        }

        for mut bus_factor in stats.bus_factors {
            bus_factor.directory = format!("{project}/{}", bus_factor.directory);
            out.bus_factors.push(bus_factor);
        }

        out.tags.extend(stats.tags);
    }

    out.commits.sort_by_key(|c| std::cmp::Reverse(c.timestamp));
    out.bus_factors.sort_by_key(|a| a.bus_factor);
    out.tags.sort_by_key(|t| std::cmp::Reverse(t.timestamp));

    let total = out.total_commits.max(1) as f64;
    let healthy = (out.commit_size_distribution[0]
        + out.commit_size_distribution[1]
        + out.commit_size_distribution[2]) as f64;
    out.cohesion_score = (healthy / total * 100.0).round() as u8;

    out.from_ref = String::new();
    out.to_ref = project_names.join(", ");

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::BusFactorEntry;

    fn export(dir: &Path, project: &str, stats: RepoStats) -> std::path::PathBuf {
        let path = dir.join(format!("{project}.json"));
        let export = ProjectStats {
            project: project.to_string(),
            stats,
        };
        fs::write(&path, serde_json::to_string(&export).unwrap()).unwrap();
        path
    }

    fn author(name: &str, email: &str, commits: u64, is_bot: bool) -> AuthorStats {
        AuthorStats {
            name: name.to_string(),
            email: email.to_string(),
            commits,
            is_bot,
            ..Default::default()
        }
    }

    fn project(authors: Vec<AuthorStats>, file: &str, commits: u64, hour: usize) -> RepoStats {
        let mut stats = RepoStats {
            total_commits: commits,
            total_additions: commits * 10,
            ..Default::default()
        };
        stats.commits_by_hour[hour] = commits;
        stats.commit_size_distribution[0] = commits;
        for a in authors {
            stats.authors.insert(a.email.clone(), a);
        }
        stats.files.insert(
            file.to_string(),
            FileStats {
                path: file.to_string(),
                ..Default::default()
            },
        );
        stats.bus_factors.push(BusFactorEntry {
            directory: "src".to_string(),
            bus_factor: 1,
            top_authors: vec![],
            total_changes: 1,
        });
        stats
    }

    #[test]
    fn merges_exports() {
        let dir = tempfile::tempdir().unwrap();
        let gtk = export(
            dir.path(),
            "gtk",
            project(
                vec![
                    author("Jane", "jane@x.org", 3, false),
                    author("Marge", "marge@x.org", 1, false),
                ],
                "README",
                4,
                9,
            ),
        );
        let mutter = export(
            dir.path(),
            "mutter",
            project(
                vec![
                    author("Jane", "jane@x.org", 2, false),
                    author("Marge", "marge@x.org", 5, true),
                ],
                "README",
                7,
                9,
            ),
        );

        let out = merge(&[gtk, mutter]).unwrap();
        assert_eq!(out.total_commits, 11);
        assert_eq!(out.total_additions, 110);
        assert_eq!(out.commits_by_hour[9], 11);
        assert_eq!(out.cohesion_score, 100);
        assert_eq!(out.from_ref, "");
        assert_eq!(out.to_ref, "gtk, mutter");

        assert_eq!(out.authors["jane@x.org"].commits, 5);
        assert!(
            out.authors["marge@x.org"].is_bot,
            "flagged as a bot in any project → a bot"
        );

        // Same path in two projects must not collide.
        let mut files: Vec<_> = out.files.keys().cloned().collect();
        files.sort();
        assert_eq!(files, ["gtk/README", "mutter/README"]);
        let mut dirs: Vec<_> = out
            .bus_factors
            .iter()
            .map(|b| b.directory.clone())
            .collect();
        dirs.sort();
        assert_eq!(dirs, ["gtk/src", "mutter/src"]);
    }

    #[test]
    fn rejects_non_exports() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bogus.json");
        fs::write(&path, "{}").unwrap();
        assert!(merge(&[path]).is_err());
    }
}
