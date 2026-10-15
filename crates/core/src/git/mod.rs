use std::path::{Path, PathBuf};
use std::process::Command;

/// 사용자 gitconfig와 무관하게 출력 형식을 고정하는 공통 인자
const GLOBAL_ARGS: &[&str] = &[
    "-c", "core.quotePath=false",
    "-c", "diff.noprefix=false",
    "-c", "diff.mnemonicPrefix=false",
];

/// 모든 `git diff` 호출에 붙인다. 색, 외부 diff 도구, textconv, prefix 설정을 무력화한다.
#[allow(dead_code)]
pub(crate) const DIFF_FLAGS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--no-relative",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--no-renames",
];

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git 실행 실패: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git {args} 실패 (exit {code:?}): {stderr}")]
    Failed { args: String, code: Option<i32>, stderr: String },
    #[error("예상하지 못한 git 출력: {0}")]
    Unexpected(String),
}

/// 읽기 전용 git CLI 래퍼. 모든 호출은 이 타입을 거친다.
#[derive(Debug, Clone)]
pub struct Git {
    cwd: PathBuf,
}

impl Git {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self { cwd: cwd.into() }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.args(GLOBAL_ARGS)
            .args(args)
            .current_dir(&self.cwd)
            // 읽기 명령이 index lock을 잡지 않게 한다 (Claude Code의 git 작업과 충돌 방지)
            .env("GIT_OPTIONAL_LOCKS", "0")
            // 파일 이름의 `*`, `[`를 glob으로 해석하지 않는다
            .env("GIT_LITERAL_PATHSPECS", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        cmd
    }

    pub fn run(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        self.run_allow(args, &[0])
    }

    pub fn run_allow(&self, args: &[&str], ok_codes: &[i32]) -> Result<Vec<u8>, GitError> {
        let out = self.command(args).output()?;
        match out.status.code() {
            Some(code) if ok_codes.contains(&code) => Ok(out.stdout),
            code => Err(GitError::Failed {
                args: args.join(" "),
                code,
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            }),
        }
    }

    pub fn run_str(&self, args: &[&str]) -> Result<String, GitError> {
        let out = self.run(args)?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }
}
