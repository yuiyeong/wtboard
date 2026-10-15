/// 모든 프로젝트에서 감시하지 않는 폴더. 이름이 같으면 깊이와 관계없이 제외한다.
/// `build`, `out`, `ios/Pods`처럼 프로젝트마다 의미가 다른 폴더는 2단계 프로필에서 다룬다.
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
