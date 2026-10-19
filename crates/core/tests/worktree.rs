mod common;

use std::path::Path;
use std::sync::Arc;

use common::{clone_on_feat_with_develop, feat_repo, gh_stub, TestRepo};
use wtboard_core::git::base::BaseSource;
use wtboard_core::profile::Profile;
use wtboard_core::store::viewed::ViewedStore;
use wtboard_core::watch::Target;
use wtboard_core::worktree::{ChangeScope, Worktree};

fn open(repo: &TestRepo) -> std::sync::Arc<Worktree> {
    Worktree::open(repo.path(), Some("main".into())).unwrap()
}

fn file(p: &str) -> Target {
    Target::File(p.into())
}

fn paths(wt: &Worktree) -> Vec<String> {
    wt.files().into_iter().map(|f| f.path).collect()
}

#[test]
fn opens_with_current_changes() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let s = open(&repo).summary();
    assert_eq!((s.branch.as_deref(), s.base.as_str(), s.changed_files), (Some("feat"), "main", 1));
    assert_eq!(s.base_source, BaseSource::Config);
}

#[test]
fn open_reports_missing_path() {
    let err = Worktree::open(Path::new("/nonexistent/repo"), None).err().unwrap();
    assert!(err.to_string().contains("/nonexistent/repo"), "{err}");
}

#[test]
fn file_edit_emits_file_scope() {
    let repo = feat_repo();
    let wt = open(&repo);
    repo.write("src/a.py", "a = 2\n");
    let ev = wt.handle(&[file("src/a.py")]).unwrap().expect("should emit");
    assert_eq!((ev.scope, ev.changed_paths), (ChangeScope::File, vec!["src/a.py".to_string()]));
    assert_eq!(paths(&wt), ["src/a.py"]);
}

#[test]
fn re_editing_a_viewed_modified_file_is_caught_by_hash() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.patch("src/a.py").unwrap(); // 화면에 보였음 → hash 저장
    repo.write("src/a.py", "a = 3\n"); // +1/-1, status M 그대로
    let ev = wt.handle(&[file("src/a.py")]).unwrap().expect("hash change should emit");
    assert_eq!(ev.changed_paths, vec!["src/a.py".to_string()]);
}

#[test]
fn events_without_real_change_are_silent() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.patch("src/a.py").unwrap();
    assert!(wt.handle(&[file("src/a.py")]).unwrap().is_none()); // touch만
    assert!(wt.handle(&[file("src/b.py")]).unwrap().is_none()); // 깨끗한 파일
    repo.git(&["add", "-A"]);
    assert!(wt.handle(&[Target::List]).unwrap().is_none()); // stage는 브랜치 뷰를 안 바꾼다
    repo.git(&["commit", "-q", "-m", "wip"]);
    assert!(wt.handle(&[Target::Base]).unwrap().is_none()); // 커밋도 마찬가지
}

#[test]
fn reverting_a_file_removes_it_from_the_list() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    repo.write("src/a.py", "a = 1\n");
    let ev = wt.handle(&[file("src/a.py")]).unwrap().unwrap();
    assert_eq!(ev.changed_paths, vec!["src/a.py".to_string()]);
    assert!(wt.files().is_empty());
}

#[test]
fn deleting_a_directory_falls_back_to_full_list() {
    let repo = feat_repo();
    let wt = open(&repo);
    repo.remove("src"); // rm -rf src: event가 폴더 경로 하나로 올 수 있다
    let ev = wt.handle(&[file("src")]).unwrap().unwrap();
    assert_eq!(ev.scope, ChangeScope::List);
    assert_eq!(paths(&wt), ["src/a.py", "src/b.py"]);
}

#[test]
fn creating_a_directory_falls_back_to_full_list() {
    let repo = feat_repo();
    let wt = open(&repo);
    repo.write("gen/models/user.g.dart", "// gen\n"); // codegen이 폴더째 생성
    let ev = wt.handle(&[file("gen")]).unwrap().unwrap();
    assert_eq!(ev.scope, ChangeScope::List);
    assert_eq!(paths(&wt), ["gen/models/user.g.dart"]);
}

#[test]
fn moving_fork_point_recomputes_from_merge_base() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    clone.git(&["checkout", "-q", "-b", "feat"]);
    clone.write("feat.txt", "f\n");
    clone.commit_all("feat");
    let wt = Worktree::open(clone.path(), None).unwrap();
    assert_eq!(wt.summary().base, "origin/main");

    upstream.write("up.txt", "u\n");
    upstream.commit_all("upstream");
    clone.git(&["fetch", "-q"]);
    clone.git(&["merge", "-q", "--no-edit", "origin/main"]);
    let ev = wt.handle(&[Target::Base]).unwrap().unwrap();
    assert_eq!(ev.scope, ChangeScope::Base);
    assert_eq!(paths(&wt), ["feat.txt"]); // up.txt는 base에도 있으므로 변경 아님
}

#[test]
fn huge_event_batches_fall_back_to_full_list() {
    // 큰 레포에서 브랜치를 바꾸면 수만 개 경로가 한 batch로 온다. 경로를 argv로 넘기면 E2BIG
    let repo = feat_repo();
    let wt = open(&repo);
    repo.write("src/a.py", "a = 2\n");
    let mut targets: Vec<Target> =
        (0..40_000).map(|i| file(&format!("generated/deeply/nested/dir/file_{i:05}.txt"))).collect();
    targets.push(file("src/a.py"));
    let ev = wt.handle(&targets).unwrap().expect("should emit");
    assert_eq!(ev.scope, ChangeScope::List);
    assert_eq!(paths(&wt), ["src/a.py"]);
}

#[test]
fn generated_and_lock_files_are_marked_collapsed() {
    let repo = TestRepo::new();
    repo.write("pubspec.yaml", "name: app\n");
    repo.write(".gitattributes", "lib/api/** linguist-generated\n");
    repo.commit_all("flutter");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("lib/user.dart", "class User {}\n");
    repo.write("lib/user.g.dart", "// gen\n");
    repo.write("lib/api/client.dart", "// gen\n");
    repo.write("pubspec.lock", "x\n");
    let wt = open(&repo);
    let got: Vec<(String, bool)> = wt.files().into_iter().map(|f| (f.path, f.collapsed)).collect();
    let want = [("lib/api/client.dart", true), ("lib/user.dart", false), ("lib/user.g.dart", true), ("pubspec.lock", true)];
    assert_eq!(got, want.map(|(p, c)| (p.to_string(), c)));
    assert_eq!(wt.summary().profile, Profile::Flutter);
}

#[test]
fn single_file_refresh_keeps_collapsed_flag() {
    let repo = TestRepo::new();
    repo.write("package.json", "{}\n");
    repo.commit_all("node");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    let wt = open(&repo);
    repo.write("package-lock.json", "{}\n");
    wt.handle(&[file("package-lock.json")]).unwrap().expect("should emit");
    assert!(wt.files()[0].collapsed);
}

/// Review Focus 4: .gitattributes를 고치면 다른 파일의 접기 판정이 바로 바뀐다
#[test]
fn editing_gitattributes_recomputes_collapsed_flags() {
    let repo = feat_repo();
    repo.write("src/gen.py", "x = 1\n");
    let wt = open(&repo);
    assert!(!wt.files().iter().any(|f| f.collapsed));
    repo.write(".gitattributes", "src/gen.py linguist-generated\n");
    let ev = wt.handle(&[file(".gitattributes")]).unwrap().expect("should emit");
    assert!(ev.changed_paths.contains(&"src/gen.py".to_string()), "{ev:?}");
    assert!(wt.files().iter().find(|f| f.path == "src/gen.py").unwrap().collapsed);
}

#[test]
fn flutter_build_dir_is_not_watched() {
    let repo = TestRepo::new();
    repo.write("pubspec.yaml", "name: app\n");
    repo.commit_all("flutter");
    let wt = open(&repo);
    let l = wt.layout();
    assert_eq!(l.classify(&l.root.join("build/app/x.txt")), Target::Ignore);
    assert_eq!(l.classify(&l.root.join("lib/build.dart")), file("lib/build.dart"));
}

#[test]
fn summary_reports_last_change_time_and_fsmonitor() {
    let repo = feat_repo();
    let wt = open(&repo);
    let s = wt.summary();
    assert_eq!((s.last_changed_at, s.slow_list_ms, s.fsmonitor, s.error), (None, None, false, None));
    repo.write("src/a.py", "a = 2\n");
    wt.handle(&[file("src/a.py")]).unwrap().expect("should emit");
    assert!(wt.summary().last_changed_at.is_some());
    // `git config --get`만 하므로 fsmonitor daemon은 뜨지 않는다
    repo.git(&["config", "core.fsmonitor", "true"]);
    assert!(wt.summary().fsmonitor);
}

#[test]
fn editing_gitignore_drops_newly_ignored_files() {
    let repo = feat_repo();
    repo.write("dist/out.js", "x\n");
    let wt = open(&repo);
    assert_eq!(paths(&wt), ["dist/out.js"]);
    repo.write(".gitignore", "dist/\n");
    wt.handle(&[file(".gitignore")]).unwrap().expect("should emit");
    assert_eq!(paths(&wt), [".gitignore"]);
}

#[test]
fn pr_base_replaces_origin_head_and_emits_base_scope() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), None, gh, Arc::new(ViewedStore::in_memory())).unwrap();
    assert_eq!(wt.summary().base_source, BaseSource::Origin);

    assert_eq!(wt.claim_pr_lookup().as_deref(), Some("feat"));
    assert_eq!(wt.claim_pr_lookup(), None, "같은 브랜치는 다시 묻지 않는다");
    let ev = wt.refresh_pr_base("feat").unwrap().expect("base changed");
    assert_eq!(ev.scope, ChangeScope::Base);
    let s = wt.summary();
    assert_eq!((s.base.as_str(), s.base_source), ("origin/develop", BaseSource::Pr));

    clone.git(&["checkout", "-q", "-b", "feat2"]);
    assert_eq!(wt.claim_pr_lookup().as_deref(), Some("feat2"), "브랜치가 바뀌면 다시 묻는다");
}

#[test]
fn pr_base_without_local_remote_ref_keeps_origin_head() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("release"); // origin/release 없음
    let wt = Worktree::open_with(clone.path(), None, gh, Arc::new(ViewedStore::in_memory())).unwrap();
    assert!(wt.claim_pr_lookup().is_some());
    assert_eq!(wt.refresh_pr_base("feat").unwrap(), None);
    assert_eq!(wt.summary().base_source, BaseSource::Origin);
}

#[test]
fn configured_base_never_asks_gh() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), Some("origin/main".into()), gh, Arc::new(ViewedStore::in_memory())).unwrap();
    assert_eq!(wt.claim_pr_lookup(), None);
}

/// Review Focus 5: detached HEAD에는 PR이 없다
#[test]
fn detached_head_never_asks_gh() {
    let (_up, clone) = clone_on_feat_with_develop();
    clone.git(&["checkout", "-q", "--detach"]);
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), None, gh, Arc::new(ViewedStore::in_memory())).unwrap();
    assert_eq!(wt.claim_pr_lookup(), None);
}

/// final review #2: 브랜치를 바꾸는 사이 늦게 온 gh 결과가 새 브랜치의 base가 되면 안 된다
#[test]
fn late_pr_result_for_a_previous_branch_is_discarded() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), None, gh, Arc::new(ViewedStore::in_memory())).unwrap();
    assert_eq!(wt.claim_pr_lookup().as_deref(), Some("feat"));
    clone.git(&["checkout", "-q", "-b", "feat2"]);
    assert_eq!(wt.claim_pr_lookup().as_deref(), Some("feat2"));

    // feat를 위해 물었던 결과가 이제야 도착했다
    assert_eq!(wt.refresh_pr_base("feat").unwrap(), None);
    assert_eq!(wt.summary().base_source, BaseSource::Origin);
}
