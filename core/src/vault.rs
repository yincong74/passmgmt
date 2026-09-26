//! vault: 数据模型、加密与文件存取。

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

/// 一条密码记录。JSON 字段名保持 snake_case（spec §5.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize)]
pub struct Entry {
    pub id: String,
    pub app_name: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Entry {
    pub fn new(
        app_name: String,
        username: String,
        password: String,
        url: String,
        notes: String,
        now: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            app_name,
            username,
            password,
            url,
            notes,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 应用设置。0 表示关闭对应功能（spec §5.2）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize)]
pub struct Settings {
    pub auto_lock_minutes: u32,
    pub clipboard_clear_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_lock_minutes: 15, clipboard_clear_seconds: 30 }
    }
}

/// vault.enc 解密后的完整明文内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize)]
pub struct VaultData {
    pub entries: Vec<Entry>,
    pub settings: Settings,
}

impl Default for VaultData {
    fn default() -> Self {
        Self { entries: Vec::new(), settings: Settings::default() }
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn entry_new_sets_id_and_timestamps() {
        let e = Entry::new("Gmail".into(), "me@gmail.com".into(), "pw".into(), "https://gmail.com".into(), "".into(), 1758844800);
        assert!(!e.id.is_empty());
        assert_eq!(e.created_at, 1758844800);
        assert_eq!(e.updated_at, 1758844800);
    }

    #[test]
    fn entry_ids_are_unique() {
        let a = Entry::new("A".into(), "".into(), "".into(), "".into(), "".into(), 0);
        let b = Entry::new("B".into(), "".into(), "".into(), "".into(), "".into(), 0);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn vault_data_json_roundtrip_with_unicode() {
        let mut data = VaultData::default();
        data.entries.push(Entry::new(
            "网银".into(),
            "用户🙂".into(),
            "pässwörd—加密".into(),
            "https://例え.jp".into(),
            "备注\n第二行\t制表".into(),
            1,
        ));
        let json = serde_json::to_string(&data).unwrap();
        let back: VaultData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, back);
        // 字段名必须是 snake_case（Global Constraints）
        assert!(json.contains("\"app_name\""));
        assert!(json.contains("\"created_at\""));
    }

    #[test]
    fn settings_defaults() {
        let s = Settings::default();
        assert_eq!(s.auto_lock_minutes, 15);
        assert_eq!(s.clipboard_clear_seconds, 30);
    }
}
