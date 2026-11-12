use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{db, StoreError};

/// main 창 rect. 값은 `window_fit::encode`의 JSON (logical point)
pub const WINDOW_KEY: &str = "window_main";
/// 값은 `Theme::as_str`
pub const THEME_KEY: &str = "theme";

/// 화면 theme. 저장 값이 없거나 모르는 값이면 dark (7단계 전 화면)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            _ => Self::Dark,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// 창 배경색 (RGB). webview가 그리기 전에 보이는 색이라 body 배경과 같아야 한다
    pub fn background(self) -> (u8, u8, u8) {
        match self {
            Self::Dark => (0x03, 0x07, 0x12),
            Self::Light => (0xff, 0xff, 0xff),
        }
    }
}

/// 앱 설정 key-value (`always_on_top` 등). 값은 문자열로 저장한다
#[derive(Debug)]
pub struct SettingsStore {
    conn: Mutex<Connection>,
}

impl SettingsStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Ok(Self { conn: Mutex::new(db::open_state(path)?) })
    }

    pub fn in_memory() -> Self {
        Self { conn: Mutex::new(db::open_memory()) }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).optional()?)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_from_setting_defaults_to_dark() {
        for v in [None, Some(""), Some("LIGHT"), Some("solarized"), Some("dark")] {
            assert_eq!(Theme::from_setting(v), Theme::Dark, "{v:?}");
        }
        assert_eq!(Theme::from_setting(Some("light")), Theme::Light);
    }

    #[test]
    fn theme_roundtrips_through_store() {
        let s = SettingsStore::in_memory();
        s.set(THEME_KEY, Theme::Light.as_str()).unwrap();
        assert_eq!(Theme::from_setting(s.get(THEME_KEY).unwrap().as_deref()), Theme::Light);
    }
}
