#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 테스트용 임시 git 레포. 사용자 global gitconfig의 영향을 받지 않는다.
pub struct TestRepo {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl TestRepo {
    /// `main` 브랜치에 `README.md`("hello\n") 커밋 하나가 있는 레포
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        fs::create_dir(&root).unwrap();
        let repo = TestRepo { _dir: dir, root };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.configure();
        repo.write("README.md", "hello\n");
        repo.commit_all("init");
        repo
    }

    /// `upstream`을 clone한 레포. `origin/HEAD` → `origin/main`
    pub fn clone_of(upstream: &TestRepo) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("clone");
        run(dir.path(), &["clone", "-q", upstream.root.to_str().unwrap(), root.to_str().unwrap()]);
        let repo = TestRepo { _dir: dir, root };
        repo.configure();
        repo
    }

    fn configure(&self) {
        self.git(&["config", "user.email", "test@example.com"]);
        self.git(&["config", "user.name", "test"]);
        self.git(&["config", "commit.gpgsign", "false"]);
        self.git(&["config", "core.hooksPath", "/dev/null"]);
    }

    /// tempdir 원래 경로. macOS에서는 canonicalize되지 않은 `/var/...` 형태다.
    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn git(&self, args: &[&str]) -> String {
        run(&self.root, args)
    }

    pub fn write(&self, rel: &str, contents: impl AsRef<[u8]>) {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, contents).unwrap();
    }

    pub fn remove(&self, rel: &str) {
        let p = self.root.join(rel);
        if p.is_dir() {
            fs::remove_dir_all(p).unwrap();
        } else {
            fs::remove_file(p).unwrap();
        }
    }

    pub fn commit_all(&self, msg: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg]);
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
}

fn run(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}
