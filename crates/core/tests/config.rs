use std::path::Path;

use wtboard_core::store::config::{expand_tilde, Config};

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
