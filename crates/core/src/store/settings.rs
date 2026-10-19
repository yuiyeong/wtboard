use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use super::{db, StoreError};

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
