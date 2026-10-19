use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use wtboard_core::domain::MAX_DIFF_BYTES;
use wtboard_core::session::{
    is_valid_session_id, valid_abs_path, BaselineContent, BaselineKind, Event, EventKind, Meta,
    SessionDir, Snapshot,
};
use wtboard_core::store::config::Config;

fn meta() -> Meta {
    Meta {
        started_at: 1_000,
        project_dir: PathBuf::from("/Users/me/dev/app"),
        start_head: Some("abc123".into()),
        turn: 2,
        turn_open: true,
        ended_at: None,
    }
}

fn event(turn: u32, path: &str) -> Event {
    Event {
        t: 2_000,
        turn,
        kind: EventKind::Edit,
        tool: Some("Edit".into()),
        paths: vec![PathBuf::from(path)],
        agent_id: None,
    }
}

fn hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn set_mtime(path: &Path, ms: u64) {
    let f = fs::File::options().write(true).open(path).unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_millis(ms)).unwrap();
}

#[test]
fn rejects_bad_session_ids() {
    let data = tempfile::tempdir().unwrap();
    for bad in ["../x", "a/b", "", "a b", ".", ".."] {
        assert!(!is_valid_session_id(bad), "{bad:?}");
        assert!(SessionDir::open(data.path(), bad).is_none(), "{bad:?}");
    }
    let dir = SessionDir::open(data.path(), "abc-123_X").unwrap();
    assert_eq!(dir.id(), "abc-123_X");
    assert_eq!(dir.root(), data.path().join("sessions/abc-123_X"));
    // 여는 것만으로는 폴더를 만들지 않는다
    assert!(!dir.root().exists());
}

#[test]
fn valid_abs_path_rules() {
    assert_eq!(valid_abs_path(Path::new("rel/x")), None);
    assert_eq!(valid_abs_path(Path::new("/a/../b")), None);
    assert_eq!(valid_abs_path(Path::new("/a/b")), Some(PathBuf::from("/a/b")));
    let nfd = "/a/\u{1112}\u{1161}\u{11AB}";
    assert_eq!(valid_abs_path(Path::new(nfd)), Some(PathBuf::from("/a/한")));
}

#[test]
fn meta_roundtrip_and_atomic() {
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    assert_eq!(dir.read_meta(), None);
    dir.write_meta(&meta()).unwrap();
    let mut m = meta();
    m.turn = 3;
    m.ended_at = Some(5_000);
    dir.write_meta(&m).unwrap();
    assert_eq!(dir.read_meta(), Some(m));
    let names: Vec<_> =
        fs::read_dir(dir.root()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, vec!["meta.json"]);
}

#[test]
fn events_skip_broken_lines() {
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    assert!(dir.events().is_empty());
    dir.append_event(&event(1, "/a/x.py")).unwrap();
    let mut f = fs::File::options().append(true).open(dir.root().join("events.jsonl")).unwrap();
    std::io::Write::write_all(&mut f, b"{\"t\": broken\n").unwrap();
    let turn_start = Event {
        t: 3_000,
        turn: 2,
        kind: EventKind::TurnStart,
        tool: None,
        paths: vec![],
        agent_id: Some("agent-1".into()),
    };
    dir.append_event(&turn_start).unwrap();
    assert_eq!(dir.events(), vec![event(1, "/a/x.py"), turn_start]);
    let raw = fs::read_to_string(dir.root().join("events.jsonl")).unwrap();
    assert!(raw.lines().next().unwrap().contains("\"kind\":\"edit\""), "{raw}");
    assert!(raw.contains("\"kind\":\"turn_start\""), "{raw}");
}

#[test]
fn baseline_first_writer_wins() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let src = work.path().join("a.py");
    fs::write(&src, "v1\n").unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();

    assert!(dir.ensure_baseline(&src, BaselineContent::CopyOf(&src)).unwrap());
    assert!(!dir.ensure_baseline(&src, BaselineContent::Bytes(b"v2\n")).unwrap());

    let file = dir.baseline_file(&src);
    assert!(file.starts_with(dir.root().join("baseline")));
    assert!(file.ends_with(src.strip_prefix("/").unwrap()));
    assert_eq!(fs::read_to_string(file).unwrap(), "v1\n");
    let all = dir.baselines();
    assert_eq!(all.len(), 1);
    assert_eq!(all[&src].kind, BaselineKind::File);
    assert_eq!(all[&src].head, None);
}

#[test]
fn baseline_race_single_winner() {
    let data = tempfile::tempdir().unwrap();
    let dir = Arc::new(SessionDir::open(data.path(), "s1").unwrap());
    let path = PathBuf::from("/x/race.py");
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let (dir, path) = (dir.clone(), path.clone());
            std::thread::spawn(move || {
                let bytes = format!("v{i}\n");
                dir.ensure_baseline(&path, BaselineContent::Bytes(bytes.as_bytes())).unwrap()
            })
        })
        .collect();
    let wins = handles.into_iter().map(|h| h.join().unwrap()).filter(|w| *w).count();
    assert_eq!(wins, 1);
    assert_eq!(dir.baselines().len(), 1);
    let lines = fs::read_to_string(dir.root().join("baseline.jsonl")).unwrap();
    assert_eq!(lines.lines().count(), 1);
}

#[test]
fn baseline_kinds() {
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    let (new, unc, unc_empty, big) =
        (Path::new("/w/new.py"), Path::new("/w/b.py"), Path::new("/w/c.py"), Path::new("/w/big.bin"));
    assert!(dir.ensure_baseline(new, BaselineContent::New).unwrap());
    let uncertain =
        BaselineContent::Uncertain { head: Some("abc".into()), bytes: Some(b"head b\n") };
    assert!(dir.ensure_baseline(unc, uncertain).unwrap());
    let no_bytes = BaselineContent::Uncertain { head: None, bytes: None };
    assert!(dir.ensure_baseline(unc_empty, no_bytes).unwrap());
    let large = vec![b'x'; MAX_DIFF_BYTES as usize + 1];
    assert!(dir.ensure_baseline(big, BaselineContent::Bytes(&large)).unwrap());

    let all = dir.baselines();
    assert_eq!(all[new].kind, BaselineKind::New);
    assert_eq!(all[unc].kind, BaselineKind::Uncertain);
    assert_eq!(all[unc].head.as_deref(), Some("abc"));
    assert_eq!(fs::read_to_string(dir.baseline_file(unc)).unwrap(), "head b\n");
    assert_eq!(all[unc_empty].kind, BaselineKind::Uncertain);
    assert_eq!(fs::read(dir.baseline_file(unc_empty)).unwrap_or_default(), b"");
    assert_eq!(all[big].kind, BaselineKind::TooLarge);
    assert_eq!(all[big].size, Some(large.len() as u64));
    assert_eq!(all[big].hash.as_deref(), Some(hex(&large).as_str()));
    assert_eq!((all[unc].size, all[unc].hash.as_deref()), (None, None));
    // 큰 파일은 내용을 복사하지 않는다
    assert!(fs::metadata(dir.baseline_file(big)).map(|m| m.len()).unwrap_or(0) == 0);

    let raw = fs::read_to_string(dir.root().join("baseline.jsonl")).unwrap();
    assert!(raw.contains("\"kind\":\"too_large\""), "{raw}");
}

#[test]
fn baseline_copy_of_large_file_is_too_large() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let src = work.path().join("big.bin");
    let large = vec![b'x'; MAX_DIFF_BYTES as usize + 1];
    fs::write(&src, &large).unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    assert!(dir.ensure_baseline(&src, BaselineContent::CopyOf(&src)).unwrap());
    let b = &dir.baselines()[&src];
    assert_eq!(b.kind, BaselineKind::TooLarge);
    assert_eq!(b.size, Some(large.len() as u64));
    assert_eq!(b.hash.as_deref(), Some(hex(&large).as_str()));
}

#[test]
fn baseline_rejects_relative_and_traversal_paths() {
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    assert!(dir.ensure_baseline(Path::new("rel.py"), BaselineContent::New).is_err());
    assert!(dir.ensure_baseline(Path::new("/a/../../etc/x"), BaselineContent::New).is_err());
    assert!(dir.baselines().is_empty());
    // 잘못된 경로도 세션 폴더 밖을 가리키지 않는다
    assert!(dir.baseline_file(Path::new("/a/../../../etc/x")).starts_with(dir.root()));
}

#[test]
fn turn_snapshot_content_deleted_too_large() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (a, gone, big) =
        (work.path().join("a.py"), work.path().join("gone.py"), work.path().join("big.bin"));
    fs::write(&a, "v2\n").unwrap();
    let large = vec![b'y'; MAX_DIFF_BYTES as usize + 7];
    fs::write(&big, &large).unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();

    dir.snapshot_turn(1, &[a.clone(), gone.clone(), big.clone()]).unwrap();
    let names: Vec<_> = fs::read_dir(dir.root().join("turns/1"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".txt") || n.ends_with(".jsonl"))
        .collect();
    assert_eq!(names, vec!["index.jsonl"]);

    match dir.turn_snapshot(1, &a) {
        Some(Snapshot::Content(p)) => {
            assert!(p.starts_with(dir.root().join("turns/1")));
            assert_eq!(fs::read_to_string(p).unwrap(), "v2\n");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(dir.turn_snapshot(1, &gone), Some(Snapshot::Deleted));
    assert_eq!(
        dir.turn_snapshot(1, &big),
        Some(Snapshot::TooLarge { size: large.len() as u64, hash: hex(&large) })
    );
    assert_eq!(dir.turn_snapshot(1, &work.path().join("other.py")), None);
    assert_eq!(dir.turn_snapshot(2, &a), None);

    // 같은 turn을 다시 찍으면 그 시점 상태로 바뀐다
    fs::remove_file(&a).unwrap();
    dir.snapshot_turn(1, std::slice::from_ref(&a)).unwrap();
    assert_eq!(dir.turn_snapshot(1, &a), Some(Snapshot::Deleted));
}

#[test]
fn snapshot_turn_isolates_bad_paths() {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let w = work.path();
    fs::create_dir(w.join("d")).unwrap();
    fs::write(w.join("d/x.py"), "x\n").unwrap();
    fs::write(w.join("a.py"), "a\n").unwrap();
    fs::write(w.join("locked.py"), "secret\n").unwrap();
    fs::set_permissions(w.join("locked.py"), fs::Permissions::from_mode(0o000)).unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();

    // d/x.py가 먼저 와서 turns/1/.../d 가 폴더가 된 뒤 d 자체(폴더)가 온다
    let paths = [
        PathBuf::from("/"),
        w.join("d/x.py"),
        w.join("d"),
        w.join("locked.py"),
        w.join("gone.py"),
        w.join("a.py"),
    ];
    let result = dir.snapshot_turn(1, &paths);
    fs::set_permissions(w.join("locked.py"), fs::Permissions::from_mode(0o644)).unwrap();
    result.unwrap();

    let content = |p: &Path| match dir.turn_snapshot(1, p) {
        Some(Snapshot::Content(f)) => fs::read_to_string(f).unwrap(),
        other => panic!("{p:?}: {other:?}"),
    };
    assert_eq!(content(&w.join("d/x.py")), "x\n");
    assert_eq!(content(&w.join("a.py")), "a\n");
    assert_eq!(dir.turn_snapshot(1, &w.join("d")), Some(Snapshot::Deleted));
    assert_eq!(dir.turn_snapshot(1, &w.join("gone.py")), Some(Snapshot::Deleted));
    assert!(dir.root().join("turns/1/index.jsonl").is_file());
}

#[test]
fn turn_index_is_not_spoofed_by_newline_in_path() {
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    dir.snapshot_turn(1, &[PathBuf::from("/nope/x\n/nope/y")]).unwrap();
    assert_eq!(dir.turn_snapshot(1, Path::new("/nope/x\n/nope/y")), Some(Snapshot::Deleted));
    assert_eq!(dir.turn_snapshot(1, Path::new("/nope/y")), None);
}

#[test]
fn append_event_keeps_only_valid_paths() {
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    let nfd = "/a/\u{1112}\u{1161}\u{11AB}";
    let mut ev = event(1, "rel/x");
    ev.paths.extend([PathBuf::from("/a/../b"), PathBuf::from(nfd), PathBuf::from("/ok")]);
    dir.append_event(&ev).unwrap();
    // 남는 경로가 없는 Edit·Bash event는 쓰지 않는다
    dir.append_event(&event(1, "rel/only")).unwrap();
    let mut bash = event(1, "/x/../y");
    bash.kind = EventKind::Bash;
    dir.append_event(&bash).unwrap();
    // turn event는 경로가 없어도 쓴다
    let turn_end = Event { kind: EventKind::TurnEnd, tool: None, paths: vec![], ..event(1, "/") };
    dir.append_event(&turn_end).unwrap();

    let events = dir.events();
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[0].paths, vec![PathBuf::from("/a/한"), PathBuf::from("/ok")]);
    assert_eq!(events[1], turn_end);
}

#[test]
fn last_activity_is_events_mtime() {
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    assert_eq!(dir.last_activity(), None);
    dir.write_meta(&meta()).unwrap();
    set_mtime(&dir.root().join("meta.json"), 1_700_000_000_000);
    assert_eq!(dir.last_activity(), Some(1_700_000_000_000));
    dir.append_event(&event(1, "/a/x.py")).unwrap();
    set_mtime(&dir.root().join("events.jsonl"), 1_700_000_500_000);
    assert_eq!(dir.last_activity(), Some(1_700_000_500_000));
}

#[test]
fn config_sessions_keep_days_default() {
    let cfg = Config::parse("[[repo]]\npath = \"/x\"\n").unwrap();
    assert_eq!(cfg.sessions.keep_days, 7);
    let cfg = Config::parse("[sessions]\nkeep_days = 3\n").unwrap();
    assert_eq!(cfg.sessions.keep_days, 3);
}

/// `root`와 그 아래 폴더는 0700, 파일은 0600이어야 한다
fn assert_private_tree(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(root), 0o700, "{}", root.display());
    let mut stack = vec![root.to_path_buf()];
    let mut files = 0;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                assert_eq!(mode(&p), 0o700, "{}", p.display());
                stack.push(p);
            } else {
                assert_eq!(mode(&p), 0o600, "{}", p.display());
                files += 1;
            }
        }
    }
    assert!(files > 0);
}

#[test]
fn session_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let secret = work.path().join("cfg/.env");
    fs::create_dir_all(secret.parent().unwrap()).unwrap();
    fs::write(&secret, "TOKEN=1\n").unwrap();
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
    let big = work.path().join("big.bin");
    fs::write(&big, vec![0u8; MAX_DIFF_BYTES as usize + 1]).unwrap();

    let dir = SessionDir::open(data.path(), "s1").unwrap();
    dir.write_meta(&meta()).unwrap();
    dir.append_event(&event(1, secret.to_str().unwrap())).unwrap();
    dir.ensure_baseline(&secret, BaselineContent::CopyOf(&secret)).unwrap();
    dir.ensure_baseline(&big, BaselineContent::CopyOf(&big)).unwrap();
    dir.ensure_baseline(&work.path().join("new.txt"), BaselineContent::New).unwrap();
    dir.snapshot_turn(1, &[secret.clone(), big.clone()]).unwrap();
    // 같은 turn을 다시 찍어도 (덮어쓰기) 그대로
    dir.snapshot_turn(1, &[secret.clone()]).unwrap();

    assert_private_tree(&data.path().join("sessions"));
    let Some(Snapshot::Content(snap)) = dir.turn_snapshot(1, &secret) else { panic!() };
    assert_eq!(fs::read_to_string(snap).unwrap(), "TOKEN=1\n");
}
