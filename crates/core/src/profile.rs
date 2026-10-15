use std::path::Path;

use serde::Serialize;

/// 모든 프로젝트에서 감시하지 않는 폴더. 이름이 같으면 깊이와 관계없이 제외한다.
pub const COMMON_EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".venv",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
    ".mypy_cache",
    ".next",
    ".turbo",
    ".dart_tool",
    ".fvm",
];

/// worktree root의 marker 파일로 정하는 프로젝트 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Flutter,
    Node,
    Python,
    Default,
}

impl Profile {
    /// 위에서부터 처음 맞는 것. Flutter 레포에 `package.json`이 있어도 Flutter다
    pub fn detect(root: &Path) -> Self {
        let has = |name: &str| root.join(name).is_file();
        if has("pubspec.yaml") {
            Self::Flutter
        } else if has("package.json") {
            Self::Node
        } else if has("pyproject.toml") || has("requirements.txt") {
            Self::Python
        } else {
            Self::Default
        }
    }

    /// 감시에서 뺄 폴더 pattern. `/`가 있으면 연속된 segment로 맞춘다 (`ios/Pods`)
    pub fn excluded_dirs(self) -> Vec<&'static str> {
        let own: &[&str] = match self {
            Self::Flutter => &[".dart_tool", "build", "ios/Pods", ".fvm"],
            Self::Node => &["node_modules", ".next", "out", ".turbo"],
            Self::Python => &["venv", ".venv", "__pycache__", ".pytest_cache", ".ruff_cache", ".mypy_cache"],
            Self::Default => &[],
        };
        COMMON_EXCLUDED_DIRS.iter().chain(own).copied().collect()
    }

    /// 생성·lock 파일이라 접어서 보여 줄 파일인지. 파일 이름(마지막 segment)에 glob으로 맞춘다
    pub fn collapses(self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.collapse_patterns().iter().any(|p| glob_name(p, name))
    }

    fn collapse_patterns(self) -> &'static [&'static str] {
        match self {
            Self::Flutter => &[
                "*.g.dart",
                "*.freezed.dart",
                "pubspec.lock",
                "Podfile.lock",
                "GeneratedPluginRegistrant.*",
            ],
            Self::Node => &["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "next-env.d.ts"],
            Self::Python => &["uv.lock", "poetry.lock"],
            Self::Default => &[],
        }
    }
}

/// `*.g.dart`(끝), `GeneratedPluginRegistrant.*`(시작), 그 외는 이름 전체
fn glob_name(pattern: &str, name: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

/// 경로 segment 안에 제외 pattern의 segment가 연속으로 나오는지. 깊이는 상관없다
pub fn is_excluded(parts: &[&str], patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        let pat: Vec<&str> = p.split('/').collect();
        parts.windows(pat.len()).any(|w| w == pat.as_slice())
    })
}
