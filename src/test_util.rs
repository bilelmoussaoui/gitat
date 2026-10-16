//! Throwaway git repositories for tests, built with the `git` CLI so commit dates and
//! authors are exactly what the test says.

use std::fs;
use std::path::Path;
use std::process::Command;

use gix::ObjectId;
use tempfile::TempDir;

pub struct TestRepo {
    pub dir: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let repo = TestRepo {
            dir: tempfile::tempdir().unwrap(),
        };
        repo.git(&["init", "-q", "-b", "main"], &[]);
        repo
    }

    /// `git clone --depth <depth>` of this repo: its oldest commit has parents that aren't there.
    pub fn shallow_clone(&self, depth: u32) -> TestRepo {
        let clone = TestRepo {
            dir: tempfile::tempdir().unwrap(),
        };
        let url = format!("file://{}", self.path().display());
        clone.git(
            &["clone", "-q", "--depth", &depth.to_string(), &url, "."],
            &[],
        );
        clone
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn open(&self) -> gix::Repository {
        gix::open(self.path()).unwrap()
    }

    fn git(&self, args: &[&str], env: &[(&str, &str)]) -> String {
        let out = Command::new("git")
            .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
            .args(args)
            .current_dir(self.path())
            // Keep the user's/CI's global config (hooks, signing, ...) out of it.
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            // CI runners have no identity configured; commits override these.
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.org")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.org")
            .envs(env.iter().copied())
            .output()
            .expect("failed to run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// Write `files` (path, contents) and commit them as `author` ("Name <email>") at `date`
    /// (anything git accepts, e.g. `2024-01-10T12:00:00Z`), used as both author and committer date.
    pub fn commit(
        &self,
        files: &[(&str, &str)],
        message: &str,
        author: &str,
        date: &str,
    ) -> ObjectId {
        for (path, contents) in files {
            let full = self.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, contents).unwrap();
        }
        self.git(&["add", "-A"], &[]);
        let (name, email) = author
            .strip_suffix('>')
            .and_then(|a| a.split_once(" <"))
            .expect("author must look like 'Name <email>'");
        let env = [
            ("GIT_AUTHOR_NAME", name),
            ("GIT_AUTHOR_EMAIL", email),
            ("GIT_COMMITTER_NAME", name),
            ("GIT_COMMITTER_EMAIL", email),
            ("GIT_AUTHOR_DATE", date),
            ("GIT_COMMITTER_DATE", date),
        ];
        self.git(&["commit", "-q", "--allow-empty", "-m", message], &env);
        self.rev("HEAD")
    }

    pub fn tag(&self, name: &str) {
        self.git(&["tag", name], &[]);
    }

    pub fn checkout(&self, args: &[&str]) {
        let mut full = vec!["checkout", "-q"];
        full.extend_from_slice(args);
        self.git(&full, &[]);
    }

    /// `git merge --no-ff branch` into the current branch, committed at `date`.
    pub fn merge(&self, branch: &str, date: &str) -> ObjectId {
        let env = [("GIT_AUTHOR_DATE", date), ("GIT_COMMITTER_DATE", date)];
        let msg = format!("Merge {branch}");
        self.git(&["merge", "-q", "--no-ff", "-m", &msg, branch], &env);
        self.rev("HEAD")
    }

    pub fn rev(&self, spec: &str) -> ObjectId {
        ObjectId::from_hex(self.git(&["rev-parse", spec], &[]).as_bytes()).unwrap()
    }
}

pub const ALICE: &str = "Alice <alice@example.org>";
pub const BOB: &str = "Bob <bob@redhat.com>";
pub const MARGE: &str = "Marge Bot <marge@example.org>";
