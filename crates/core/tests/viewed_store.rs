use std::collections::HashMap;

use wtboard_core::store::viewed::ViewedStore;

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn set_then_load() {
    let s = ViewedStore::in_memory();
    s.set("w", "feat", "a.py", "h1").unwrap();
    assert_eq!(s.load("w", "feat").unwrap(), map(&[("a.py", "h1")]));
}

#[test]
fn set_overwrites_hash() {
    let s = ViewedStore::in_memory();
    s.set("w", "feat", "a.py", "h1").unwrap();
    s.set("w", "feat", "a.py", "h2").unwrap();
    assert_eq!(s.load("w", "feat").unwrap(), map(&[("a.py", "h2")]));
}

#[test]
fn remove_deletes_row() {
    let s = ViewedStore::in_memory();
    s.set("w", "feat", "a.py", "h1").unwrap();
    s.remove("w", "feat", "a.py").unwrap();
    assert!(s.load("w", "feat").unwrap().is_empty());
}

#[test]
fn rows_are_split_by_worktree_and_branch() {
    let s = ViewedStore::in_memory();
    s.set("w", "feat", "a.py", "h1").unwrap();
    s.set("w", "main", "a.py", "h2").unwrap();
    s.set("v", "feat", "a.py", "h3").unwrap();
    assert_eq!(s.load("w", "feat").unwrap(), map(&[("a.py", "h1")]));
    assert_eq!(s.load("w", "main").unwrap(), map(&[("a.py", "h2")]));
    assert_eq!(s.load("v", "feat").unwrap(), map(&[("a.py", "h3")]));
}

#[test]
fn survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let s = ViewedStore::open(&path).unwrap();
    s.set("w", "feat", "a.py", "h1").unwrap();
    drop(s);
    let s = ViewedStore::open(&path).unwrap();
    assert_eq!(s.load("w", "feat").unwrap(), map(&[("a.py", "h1")]));
}

#[test]
fn open_creates_parent_dir() {
    let dir = tempfile::tempdir().unwrap();
    ViewedStore::open(&dir.path().join("nested/state.sqlite")).unwrap();
}

#[test]
fn open_fails_under_a_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file"), "").unwrap();
    assert!(ViewedStore::open(&dir.path().join("file/state.sqlite")).is_err());
}
