use serde::Serialize;

/// 이보다 큰 파일은 diff 대신 크기 변화만 보여 준다
pub const MAX_DIFF_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,
    /// `None`이면 바이너리이거나, `MAX_DIFF_BYTES`를 넘어 줄 수를 세지 않은 untracked 파일
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}
