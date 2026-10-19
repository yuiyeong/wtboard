//! wtboard 공용 레이어: git 읽기, diff 계산, 감시, 설정, 세션 기록.
pub mod gh;
pub mod git;
pub mod store;
pub mod domain;
pub mod paths;
pub mod profile;
pub mod registry;
pub mod repo;
pub mod session;
pub mod watch;
pub mod worktree;
