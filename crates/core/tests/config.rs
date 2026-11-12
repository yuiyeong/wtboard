use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use wtboard_core::store::config::{config_path, expand_tilde, Config, ConfigError};
use wtboard_core::store::config_edit::{abbreviate_home, add_repo, remove_repo, set_repo_base, update_file};

#[test]
fn parses_repos_and_fills_defaults() {
    let cfg = Config::parse(
        r#"
[[repo]]
path = "~/dev/my-api"
base = "origin/main"

[[repo]]
path = "/abs/my-app"
"#,
    )
    .unwrap();
    assert_eq!(cfg.repos.len(), 2);
    assert_eq!(cfg.repos[0].base.as_deref(), Some("origin/main"));
    assert_eq!(cfg.repos[1].base, None);
    assert_eq!(cfg.watch.fallback_poll_secs, 30);
}

#[test]
fn ignores_sections_of_later_stages() {
    let cfg = Config::parse(
        "[sessions]\nkeep_days = 7\n[review]\nauto_attach_comments = true\n[[repo]]\npath = \"/x\"\n",
    )
    .unwrap();
    assert_eq!(cfg.repos.len(), 1);
}

#[test]
fn expands_tilde_only_at_start() {
    let home = Path::new("/Users/me");
    assert_eq!(expand_tilde("~/dev/a", home), Path::new("/Users/me/dev/a"));
    assert_eq!(expand_tilde("~", home), Path::new("/Users/me"));
    assert_eq!(expand_tilde("/abs/~x", home), Path::new("/abs/~x"));
}

#[test]
fn load_names_the_missing_file() {
    let err = Config::load(Path::new("/nonexistent/config.toml")).unwrap_err();
    assert!(err.to_string().contains("/nonexistent/config.toml"), "{err}");
}

#[test]
fn wtboard_config_env_overrides_default_path() {
    let home = Path::new("/Users/me");
    assert_eq!(config_path(Some(OsString::from("/tmp/x.toml")), home), Path::new("/tmp/x.toml"));
    assert_eq!(config_path(Some(OsString::new()), home), Path::new("/Users/me/.wtboard/config.toml"));
    assert_eq!(config_path(None, home), Path::new("/Users/me/.wtboard/config.toml"));
}

#[test]
fn poll_interval_has_a_floor_of_five_seconds() {
    let zero = Config::parse("[watch]\nfallback_poll_secs = 0\n").unwrap();
    assert_eq!(zero.watch.poll_interval(), Duration::from_secs(5));
    let minute = Config::parse("[watch]\nfallback_poll_secs = 60\n").unwrap();
    assert_eq!(minute.watch.poll_interval(), Duration::from_secs(60));
}

#[test]
fn review_auto_attach_default_true() {
    assert!(Config::parse("").unwrap().review.auto_attach_comments);
    let off = Config::parse("[review]\nauto_attach_comments = false\n").unwrap();
    assert!(!off.review.auto_attach_comments);
    assert!(Config::parse("[review]\n").unwrap().review.auto_attach_comments);
}

#[test]
fn hook_binary_path_follows_run_sh_rule() {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use wtboard_core::store::config::hook_binary_path;
    let home = Path::new("/Users/me");
    assert_eq!(hook_binary_path(None, home), PathBuf::from("/Users/me/.wtboard/bin/wtboard-hook"));
    assert_eq!(hook_binary_path(Some(OsString::from("")), home), PathBuf::from("/Users/me/.wtboard/bin/wtboard-hook"));
    assert_eq!(hook_binary_path(Some(OsString::from("/tmp/h")), home), PathBuf::from("/tmp/h"));
}

const SRC: &str = r#"# base 생략 → origin/HEAD
[[repo]]
path = "~/a"

[[repo]]
path = "~/b"
base = "origin/main"

[review]
auto_attach_comments = false
"#;

fn paths(cfg: &Config) -> Vec<&str> {
    cfg.repos.iter().map(|r| r.path.as_str()).collect()
}

#[test]
fn add_appends_repo_and_keeps_rest() {
    let out = add_repo(SRC, "~/c").unwrap();
    let cfg = Config::parse(&out).unwrap();
    assert_eq!(paths(&cfg), ["~/a", "~/b", "~/c"]);
    assert_eq!(cfg.repos[2].base, None);
    assert!(!cfg.review.auto_attach_comments);
    assert!(out.contains("# base 생략 → origin/HEAD"));
    let mut rest = out.lines();
    for line in SRC.lines() {
        assert!(rest.any(|l| l == line), "줄이 없거나 순서가 바뀜: {line:?}\n{out}");
    }
}

#[test]
fn add_to_empty_source() {
    let out = add_repo("", "~/c").unwrap();
    assert!(out.starts_with("[[repo]]"), "{out:?}");
    assert_eq!(paths(&Config::parse(&out).unwrap()), ["~/c"]);
}

/// 원래 내용은 한 byte도 움직이지 않고, 새 entry는 그 뒤에 온다
fn assert_pure_append(src: &str, expected: &[&str]) {
    let out = add_repo(src, "~/c").unwrap();
    assert!(out.starts_with(src), "앞부분이 바뀜:\n{out}");
    assert_eq!(paths(&Config::parse(&out).unwrap()), expected, "{out}");
}

#[test]
fn add_keeps_comment_after_last_repo_above_new_entry() {
    let src = "[[repo]]\npath = \"~/a\"\n# base = \"origin/develop\"\n";
    assert_pure_append(src, &["~/a", "~/c"]);
}

#[test]
fn add_goes_after_table_that_follows_repos() {
    assert_pure_append(SRC, &["~/a", "~/b", "~/c"]);
}

#[test]
fn add_keeps_trailing_comment_after_review() {
    let src = format!("{SRC}# 끝 주석\n");
    assert_pure_append(&src, &["~/a", "~/b", "~/c"]);
}

#[test]
fn add_to_source_without_final_newline() {
    assert_pure_append("[[repo]]\npath = \"~/a\"\n# 끝", &["~/a", "~/c"]);
    assert_pure_append("[[repo]]\npath = \"~/a\"", &["~/a", "~/c"]);
}

#[test]
fn add_escapes_path() {
    let path = "/Users/x/한글 폴더/a\"b\\c";
    let cfg = Config::parse(&add_repo(SRC, path).unwrap()).unwrap();
    assert_eq!(cfg.repos.last().unwrap().path, path);
}

#[test]
fn remove_keeps_leading_comment() {
    let out = remove_repo(SRC, "~/a").unwrap();
    let cfg = Config::parse(&out).unwrap();
    assert_eq!(paths(&cfg), ["~/b"]);
    assert_eq!(cfg.repos[0].base.as_deref(), Some("origin/main"));
    assert!(out.contains("# base 생략 → origin/HEAD"), "{out}");
    assert!(out.contains("[review]\nauto_attach_comments = false"), "{out}");
}

#[test]
fn remove_last_repo_leaves_valid_file() {
    let out = remove_repo(&remove_repo(SRC, "~/a").unwrap(), "~/b").unwrap();
    let cfg = Config::parse(&out).unwrap();
    assert!(cfg.repos.is_empty());
    assert!(!cfg.review.auto_attach_comments);
    assert!(out.contains("# base 생략 → origin/HEAD"), "{out}");
}

#[test]
fn set_base_writes_and_clears() {
    let out = set_repo_base(SRC, "~/a", Some("origin/develop")).unwrap();
    let cfg = Config::parse(&out).unwrap();
    assert_eq!(cfg.repos[0].base.as_deref(), Some("origin/develop"));
    assert_eq!(cfg.repos[1].base.as_deref(), Some("origin/main"));
    let cfg = Config::parse(&set_repo_base(SRC, "~/b", None).unwrap()).unwrap();
    assert_eq!(cfg.repos[1].base, None);
}

#[test]
fn edit_unknown_repo_is_error() {
    assert!(matches!(remove_repo(SRC, "~/zzz"), Err(ConfigError::RepoNotFound(p)) if p == "~/zzz"));
    assert!(matches!(set_repo_base(SRC, "~/zzz", None), Err(ConfigError::RepoNotFound(p)) if p == "~/zzz"));
}

#[test]
fn duplicate_path_edits_first() {
    let src = "[[repo]]\npath = \"~/a\"\nbase = \"one\"\n\n[[repo]]\npath = \"~/a\"\nbase = \"two\"\n";
    let cfg = Config::parse(&remove_repo(src, "~/a").unwrap()).unwrap();
    assert_eq!(paths(&cfg), ["~/a"]);
    assert_eq!(cfg.repos[0].base.as_deref(), Some("two"));
}

#[test]
fn update_file_creates_missing_file_and_dir() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x/y/config.toml");
    update_file(&file, |src| add_repo(src, "~/c")).unwrap();
    assert_eq!(paths(&Config::load(&file).unwrap()), ["~/c"]);
    let names: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["config.toml"]);
}

#[test]
fn update_file_refuses_broken_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[[repo").unwrap();
    let err = update_file(&file, |src| add_repo(src, "~/c")).unwrap_err();
    assert!(matches!(err, ConfigError::Parse(_)), "{err}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "[[repo");
}

#[test]
fn update_file_writes_nothing_when_edit_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, SRC).unwrap();
    let err = update_file(&file, |src| remove_repo(src, "~/zzz")).unwrap_err();
    assert!(matches!(err, ConfigError::RepoNotFound(_)), "{err}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), SRC);
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["config.toml"]);

    let missing = dir.path().join("x/y/config.toml");
    assert!(update_file(&missing, |src| remove_repo(src, "~/zzz")).is_err());
    assert!(!dir.path().join("x").exists());
}

#[test]
fn load_or_default_missing_file_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Config::load_or_default(&dir.path().join("none.toml")).unwrap().repos.is_empty());
    let broken = dir.path().join("broken.toml");
    std::fs::write(&broken, "[[repo").unwrap();
    assert!(Config::load_or_default(&broken).is_err());
}

#[test]
fn abbreviate_home_cases() {
    let home = Path::new("/Users/jo");
    assert_eq!(abbreviate_home(Path::new("/Users/jo/p/x"), home), "~/p/x");
    assert_eq!(abbreviate_home(Path::new("/opt/x"), home), "/opt/x");
    assert_eq!(abbreviate_home(Path::new("/Users/jo"), home), "~");
    assert_eq!(abbreviate_home(Path::new("/Users/jo2/x"), home), "/Users/jo2/x");
}
