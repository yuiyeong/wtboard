use std::fs;

use wtboard_core::profile::{is_excluded, Profile};

fn root_with(files: &[&str]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for f in files {
        fs::write(d.path().join(f), "").unwrap();
    }
    d
}

#[test]
fn detects_profile_from_marker_files() {
    assert_eq!(Profile::detect(root_with(&["pubspec.yaml", "package.json"]).path()), Profile::Flutter);
    assert_eq!(Profile::detect(root_with(&["package.json"]).path()), Profile::Node);
    assert_eq!(Profile::detect(root_with(&["requirements.txt"]).path()), Profile::Python);
    assert_eq!(Profile::detect(root_with(&["pyproject.toml"]).path()), Profile::Python);
    assert_eq!(Profile::detect(root_with(&["README.md"]).path()), Profile::Default);
}

#[test]
fn excluded_patterns_match_segment_runs_at_any_depth() {
    let pats: Vec<String> = Profile::Flutter.excluded_dirs().into_iter().map(String::from).collect();
    let ex = |p: &str| is_excluded(&p.split('/').collect::<Vec<_>>(), &pats);
    assert!(ex("ios/Pods/Alamofire/x.swift"));
    assert!(ex("packages/app/ios/Pods/x"));
    assert!(ex("build/app.apk"));
    assert!(ex("node_modules/x/index.js")); // 공통 제외
    assert!(!ex("lib/ios/podsfile.dart"));
    assert!(!ex("Pods/x")); // ios 없이 Pods만
    assert!(!ex("lib/builder.dart"));
}

#[test]
fn profile_specific_dirs_apply_only_to_that_profile() {
    let has = |p: Profile, d: &str| p.excluded_dirs().contains(&d);
    assert!(has(Profile::Python, "venv"));
    assert!(!has(Profile::Node, "venv"));
    assert!(has(Profile::Node, "out"));
    assert!(!has(Profile::Flutter, "out"));
    assert!(!has(Profile::Default, "build"));
    assert!(has(Profile::Default, "node_modules"));
}

#[test]
fn collapse_patterns_match_file_names() {
    assert!(Profile::Flutter.collapses("lib/models/user.g.dart"));
    assert!(Profile::Flutter.collapses("lib/user.freezed.dart"));
    assert!(Profile::Flutter.collapses("ios/Runner/GeneratedPluginRegistrant.m"));
    assert!(Profile::Flutter.collapses("pubspec.lock"));
    assert!(Profile::Flutter.collapses("ios/Podfile.lock"));
    assert!(!Profile::Flutter.collapses("lib/user.dart"));
    assert!(Profile::Node.collapses("package-lock.json"));
    assert!(Profile::Node.collapses("apps/web/pnpm-lock.yaml"));
    assert!(Profile::Node.collapses("yarn.lock"));
    assert!(Profile::Node.collapses("next-env.d.ts"));
    assert!(!Profile::Python.collapses("package-lock.json"));
    assert!(Profile::Python.collapses("uv.lock"));
    assert!(Profile::Python.collapses("poetry.lock"));
    assert!(!Profile::Default.collapses("uv.lock"));
}
