use std::collections::HashSet;

use super::{Git, GitError};

/// `.gitattributes`에서 `linguist-generated`가 켜진 경로. GitHub PR과 같은 기준으로 생성 파일을 접는다.
pub fn linguist_generated(git: &Git, paths: &[&str]) -> Result<HashSet<String>, GitError> {
    if paths.is_empty() {
        return Ok(HashSet::new());
    }
    let input: Vec<u8> = paths.iter().flat_map(|p| p.bytes().chain([0])).collect();
    let out = git.run_with_input(&["check-attr", "-z", "--stdin", "linguist-generated"], &input)?;
    // `<path>\0<attr>\0<value>\0` 반복. value는 set / unset / unspecified / 지정한 문자열
    let fields: Vec<&[u8]> = out.split(|b| *b == 0).collect();
    Ok(fields
        .chunks(3)
        .filter_map(|c| match c {
            [path, _, b"set" | b"true"] => Some(String::from_utf8_lossy(path).into_owned()),
            _ => None,
        })
        .collect())
}
