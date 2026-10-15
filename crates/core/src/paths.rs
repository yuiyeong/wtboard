//! 경로 비교용 정규화. 같은 파일을 가리키는 경로가 같은 문자열이 되게 한다.
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

pub(crate) fn canonical(p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    nfc(&p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
}

/// macOS FSEvents는 NFD 경로를 줄 수 있고 git(core.precomposeUnicode)은 NFC를 쓴다
pub(crate) fn nfc(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().nfc().collect::<String>())
}
