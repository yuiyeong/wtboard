//! `config.toml`의 `[[repo]]`만 고친다. 사용자가 손으로 쓰는 파일이라 주석·빈 줄·다른 table은 그대로 둔다.

use std::io::Write;
use std::path::Path;

use toml_edit::{value, ArrayOfTables, DocumentMut, Item, RawString, Table, Value};

use super::config::{Config, ConfigError};

/// 파일 끝에 `[[repo]]`를 붙인다. 원래 내용은 한 byte도 바꾸지 않는다
/// (끝의 주석이 새 entry 아래로 내려가 그 entry의 것처럼 보이지 않게)
pub fn add_repo(src: &str, repo_path: &str) -> Result<String, ConfigError> {
    let doc: DocumentMut = src.parse()?;
    if doc.get("repo").is_some_and(|r| !r.is_array_of_tables()) {
        return Err(ConfigError::RepoNotFound(repo_path.into()));
    }
    let mut out = src.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.trim().is_empty() {
        out.push('\n');
    }
    // 새로 만든 Value는 decor가 없어 따옴표·escape를 포함한 TOML 문자열만 나온다
    out += &format!("[[repo]]\npath = {}\n", Value::from(repo_path));
    Ok(out)
}

pub fn remove_repo(src: &str, repo_path: &str) -> Result<String, ConfigError> {
    let mut doc: DocumentMut = src.parse()?;
    let repos = repos_mut(&mut doc, repo_path)?;
    let index = repo_index(repos, repo_path)?;
    let removed = repos.get(index).expect("방금 찾은 index");
    let position = removed.position();
    let comment = removed.decor().prefix().and_then(RawString::as_str).unwrap_or("").to_string();
    repos.remove(index);
    if repos.is_empty() {
        doc.remove("repo");
    }
    // 첫 table 앞의 주석은 그 table의 decor prefix로 들어 있어서 table과 같이 사라진다. 다음 table로 넘긴다
    if !comment.trim().is_empty() {
        match position.and_then(|pos| next_table_position(doc.as_table(), pos)) {
            Some(next) => {
                let target = table_at_mut(doc.as_table_mut(), next).expect("방금 찾은 position");
                let own = target.decor().prefix().and_then(RawString::as_str).unwrap_or("");
                let merged = if own.trim().is_empty() { comment.to_string() } else { format!("{comment}{own}") };
                target.decor_mut().set_prefix(merged);
            }
            None => {
                let merged = format!("{comment}{}", doc.trailing().as_str().unwrap_or(""));
                doc.set_trailing(merged);
            }
        }
    }
    Ok(doc.to_string())
}

/// `base`가 `None`이면 `base` 줄을 지운다 (= origin/HEAD를 따른다)
pub fn set_repo_base(src: &str, repo_path: &str, base: Option<&str>) -> Result<String, ConfigError> {
    let mut doc: DocumentMut = src.parse()?;
    let repos = repos_mut(&mut doc, repo_path)?;
    let index = repo_index(repos, repo_path)?;
    let table = repos.get_mut(index).expect("방금 찾은 index");
    match base {
        None => {
            table.remove("base");
        }
        Some(base) => match table.get_mut("base") {
            // 줄 끝 주석 같은 decor를 유지한다
            Some(item) => {
                let mut new = Value::from(base);
                if let Some(old) = item.as_value() {
                    *new.decor_mut() = old.decor().clone();
                }
                *item = Item::Value(new);
            }
            None => {
                table.insert("base", value(base));
            }
        },
    }
    Ok(doc.to_string())
}

/// 읽기(없으면 "") → `Config::parse`로 확인 → `f` → 결과도 확인 → 같은 폴더의 임시 파일에 쓰고 fsync → rename.
/// 깨진 파일에는 쓰지 않는다. 폴더가 없으면 만든다. 쓰는 도중 꺼져도 빈 파일이 남지 않는다
pub fn update_file(path: &Path, f: impl FnOnce(&str) -> Result<String, ConfigError>) -> Result<(), ConfigError> {
    // dotfiles 레포로 symlink한 config도 symlink를 지키며 target을 고친다
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => return Err(ConfigError::Io { path, source }),
    };
    Config::parse(&src)?;
    let out = f(&src)?;
    Config::parse(&out)?;

    let write_err = |source| ConfigError::Write { path: path.clone(), source };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(write_err)?;
    }
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".tmp{}", std::process::id()));
    let tmp = path.with_file_name(tmp_name);
    let write_tmp = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(out.as_bytes())?;
        // rename 뒤 꺼져도 내용이 디스크에 있게 한다
        file.sync_all()
    };
    write_tmp()
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|source| {
            let _ = std::fs::remove_file(&tmp);
            write_err(source)
        })
}

/// 홈 아래면 `~/...`로 줄여 보여 준다. `strip_prefix`라 `/Users/jo2`는 `/Users/jo`의 아래로 치지 않는다
pub fn abbreviate_home(p: &Path, home: &Path) -> String {
    match p.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

fn repos_mut<'a>(doc: &'a mut DocumentMut, repo_path: &str) -> Result<&'a mut ArrayOfTables, ConfigError> {
    doc.get_mut("repo")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| ConfigError::RepoNotFound(repo_path.into()))
}

/// 같은 path가 둘이면 첫 번째
fn repo_index(repos: &ArrayOfTables, repo_path: &str) -> Result<usize, ConfigError> {
    repos
        .iter()
        .position(|t| t.get("path").and_then(Item::as_str) == Some(repo_path))
        .ok_or_else(|| ConfigError::RepoNotFound(repo_path.into()))
}

/// 문서에서 `after` 다음에 오는 header 있는 table의 position
fn next_table_position(table: &Table, after: usize) -> Option<usize> {
    let mut best: Option<usize> = None;
    for_each_table(table, &mut |t| {
        if let Some(pos) = t.position().filter(|&p| p > after && !t.is_implicit()) {
            best = Some(best.map_or(pos, |b| b.min(pos)));
        }
    });
    best
}

fn table_at_mut(table: &mut Table, position: usize) -> Option<&mut Table> {
    if table.position() == Some(position) && !table.is_implicit() {
        return Some(table);
    }
    for (_, item) in table.iter_mut() {
        let found = match item {
            Item::Table(t) => table_at_mut(t, position),
            Item::ArrayOfTables(a) => a.iter_mut().find_map(|t| table_at_mut(t, position)),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn for_each_table(table: &Table, f: &mut impl FnMut(&Table)) {
    f(table);
    for (_, item) in table.iter() {
        match item {
            Item::Table(t) => for_each_table(t, f),
            Item::ArrayOfTables(a) => a.iter().for_each(|t| for_each_table(t, f)),
            _ => {}
        }
    }
}
