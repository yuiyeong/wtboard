use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use wtboard_core::store::config::{config_path, expand_tilde, Config};

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
