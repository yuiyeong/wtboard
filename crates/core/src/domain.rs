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
    /// 생성·lock 파일이라 접어서 보여 준다 (profile pattern 또는 `linguist-generated`)
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PatchBody {
    /// `old_source`는 old 쪽 파일 전체. 화면이 hunk 밖의 줄까지 보고 하이라이팅하는 데 쓴다. old 쪽 파일이 없으면 `None`
    Text { diff: String, old_source: Option<String> },
    Binary { old_size: Option<u64>, new_size: Option<u64> },
    TooLarge { old_size: Option<u64>, new_size: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilePatch {
    pub path: String,
    pub body: PatchBody,
    /// diff 내용의 blake3 hash. 바이너리/큰 파일은 파일 내용 기준
    pub hash: String,
}
