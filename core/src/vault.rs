//! vault: 数据模型、加密与文件存取。

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use std::path::{Path, PathBuf};
use thiserror::Error;

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

// ---------- 文件格式与加密原语（spec §5.3） ----------

pub const MAGIC: &[u8; 4] = b"PMGR";
pub const FORMAT_VERSION: u8 = 1;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;
pub const M_COST: u32 = 65536; // KiB（64 MiB）
pub const T_COST: u32 = 3;
pub const P_COST: u32 = 1;
pub const HEADER_LEN: usize = 4 + 1 + SALT_LEN + 12 + NONCE_LEN; // 45

#[derive(Debug, Error)]
pub enum VaultError {
    /// 安全要求：密码错误与文件损坏不可区分，统一用此变体
    #[error("主密码错误或数据文件已损坏")]
    WrongPasswordOrCorrupt,
    #[error("数据文件格式无效: {0}")]
    Format(String),
    #[error("文件读写失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("加密操作失败: {0}")]
    Crypto(String),
}

pub fn derive_key(
    password: &str,
    salt: &[u8; SALT_LEN],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<[u8; KEY_LEN], VaultError> {
    let params = Params::new(m_cost, t_cost, p_cost, Some(KEY_LEN))
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    Ok(key)
}

pub fn encrypt_payload(
    key: &[u8; KEY_LEN],
    plaintext: &[u8],
) -> Result<([u8; NONCE_LEN], Vec<u8>), VaultError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = Aes256Gcm::new(key.into());
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    Ok((nonce_bytes, ciphertext))
}

pub fn decrypt_payload(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| VaultError::WrongPasswordOrCorrupt)
}

pub fn encode_file(
    salt: &[u8; SALT_LEN],
    params: (u32, u32, u32),
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(FORMAT_VERSION);
    out.extend_from_slice(salt);
    out.extend_from_slice(&params.0.to_le_bytes());
    out.extend_from_slice(&params.1.to_le_bytes());
    out.extend_from_slice(&params.2.to_le_bytes());
    out.extend_from_slice(nonce);
    out.extend_from_slice(ciphertext);
    out
}

pub struct FileParts {
    pub salt: [u8; SALT_LEN],
    pub params: (u32, u32, u32),
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

pub fn decode_file(bytes: &[u8]) -> Result<FileParts, VaultError> {
    if bytes.len() < HEADER_LEN {
        return Err(VaultError::Format("文件过短".into()));
    }
    if &bytes[0..4] != MAGIC {
        return Err(VaultError::Format("不是密码管理器数据文件".into()));
    }
    if bytes[4] != FORMAT_VERSION {
        return Err(VaultError::Format(format!("不支持的版本 {}", bytes[4])));
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[5..5 + SALT_LEN]);
    let o = 5 + SALT_LEN;
    let m = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let t = u32::from_le_bytes(bytes[o + 4..o + 8].try_into().unwrap());
    let p = u32::from_le_bytes(bytes[o + 8..o + 12].try_into().unwrap());
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&bytes[o + 12..o + 12 + NONCE_LEN]);
    let ciphertext = bytes[HEADER_LEN..].to_vec();
    Ok(FileParts { salt, params: (m, t, p), nonce, ciphertext })
}

// ---------- Vault：解锁后的内存句柄 + 文件操作（spec §5.3、§6.4） ----------

/// 已解锁的 vault。Drop 时 zeroize 密钥与全部明文。
pub struct Vault {
    path: PathBuf,
    salt: [u8; SALT_LEN],
    params: (u32, u32, u32),
    key: [u8; KEY_LEN],
    pub data: VaultData,
}

impl Vault {
    /// 新建 vault 文件并立即落盘（空数据）。
    pub fn create(path: impl Into<PathBuf>, password: &str) -> Result<Self, VaultError> {
        let path = path.into();
        let mut salt = [0u8; SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let params = (M_COST, T_COST, P_COST);
        let key = derive_key(password, &salt, params.0, params.1, params.2)?;
        let mut v = Self { path, salt, params, key, data: VaultData::default() };
        v.save()?;
        Ok(v)
    }

    /// 读取并用主密码解锁。密码错误或文件损坏统一返回 WrongPasswordOrCorrupt。
    pub fn open(path: impl Into<PathBuf>, password: &str) -> Result<Self, VaultError> {
        let path = path.into();
        let bytes = std::fs::read(&path)?;
        let parts = decode_file(&bytes)?;
        let key = derive_key(password, &parts.salt, parts.params.0, parts.params.1, parts.params.2)?;
        let plaintext = decrypt_payload(&key, &parts.nonce, &parts.ciphertext)?;
        let data: VaultData =
            serde_json::from_slice(&plaintext).map_err(|_| VaultError::WrongPasswordOrCorrupt)?;
        Ok(Self { path, salt: parts.salt, params: parts.params, key, data })
    }

    /// 保存：写 .tmp → 旧文件留 .bak → 原子 rename（spec §5.3 保存流程）。
    pub fn save(&mut self) -> Result<(), VaultError> {
        let plaintext =
            serde_json::to_vec(&self.data).map_err(|e| VaultError::Crypto(e.to_string()))?;
        let (nonce, ciphertext) = encrypt_payload(&self.key, &plaintext)?;
        let bytes = encode_file(&self.salt, self.params, &nonce, &ciphertext);
        let tmp = self.path.with_extension("enc.tmp");
        std::fs::write(&tmp, &bytes)?;
        if self.path.exists() {
            std::fs::copy(&self.path, self.path.with_extension("enc.bak"))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// 修改主密码：验证旧密码 → 新 salt 重派生 → 重加密落盘（spec §5.3）。
    pub fn change_password(&mut self, old: &str, new: &str) -> Result<(), VaultError> {
        let check = derive_key(old, &self.salt, self.params.0, self.params.1, self.params.2)?;
        if check != self.key {
            return Err(VaultError::WrongPasswordOrCorrupt);
        }
        let mut salt = [0u8; SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        self.key = derive_key(new, &salt, self.params.0, self.params.1, self.params.2)?;
        self.salt = salt;
        self.save()
    }

    /// 导入备份：用备份自己的主密码验证解密 → 替换数据 → 用当前密钥重加密落盘（spec §6.4）。
    pub fn import_backup(&mut self, src: &Path, password: &str) -> Result<(), VaultError> {
        let imported = decrypt_backup(src, password)?;
        self.data = imported;
        self.save()
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        self.data.zeroize();
        self.key.zeroize();
    }
}

/// 手动实现：Debug 输出绝不包含密钥与明文数据（仅 path）。
impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("path", &self.path).finish_non_exhaustive()
    }
}

/// 导出备份 = 原样复制加密文件（永不导出明文）。
pub fn export_backup(vault_path: &Path, dest: &Path) -> Result<(), VaultError> {
    std::fs::copy(vault_path, dest)?;
    Ok(())
}

/// 验证并解密一个备份/vault 文件，返回明文数据（不改变任何文件）。
pub fn decrypt_backup(src: &Path, password: &str) -> Result<VaultData, VaultError> {
    let bytes = std::fs::read(src)?;
    let parts = decode_file(&bytes)?;
    let key = derive_key(password, &parts.salt, parts.params.0, parts.params.1, parts.params.2)?;
    let plaintext = decrypt_payload(&key, &parts.nonce, &parts.ciphertext)?;
    serde_json::from_slice(&plaintext).map_err(|_| VaultError::WrongPasswordOrCorrupt)
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

#[cfg(test)]
mod crypto_tests {
    use super::*;

    // 测试用低开销参数加速（生产参数见 Global Constraints）
    const TM: u32 = 4096;
    const TT: u32 = 1;
    const TP: u32 = 1;

    #[test]
    fn derive_key_deterministic_and_salt_sensitive() {
        let salt = [7u8; SALT_LEN];
        let k1 = derive_key("pw", &salt, TM, TT, TP).unwrap();
        let k2 = derive_key("pw", &salt, TM, TT, TP).unwrap();
        let k3 = derive_key("pw2", &salt, TM, TT, TP).unwrap();
        let k4 = derive_key("pw", &[8u8; SALT_LEN], TM, TT, TP).unwrap();
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        assert_ne!(k1, k4);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [3u8; KEY_LEN];
        let (nonce, ct) = encrypt_payload(&key, b"hello vault").unwrap();
        let pt = decrypt_payload(&key, &nonce, &ct).unwrap();
        assert_eq!(pt, b"hello vault");
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key = [3u8; KEY_LEN];
        let wrong = [4u8; KEY_LEN];
        let (nonce, ct) = encrypt_payload(&key, b"secret").unwrap();
        assert!(matches!(
            decrypt_payload(&wrong, &nonce, &ct),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [3u8; KEY_LEN];
        let (nonce, mut ct) = encrypt_payload(&key, b"secret").unwrap();
        ct[0] ^= 0xff;
        assert!(matches!(
            decrypt_payload(&key, &nonce, &ct),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
    }

    #[test]
    fn encode_decode_roundtrip() {
        let salt = [1u8; SALT_LEN];
        let nonce = [2u8; NONCE_LEN];
        let ct = vec![9u8; 64];
        let bytes = encode_file(&salt, (M_COST, T_COST, P_COST), &nonce, &ct);
        assert_eq!(bytes.len(), HEADER_LEN + 64);
        let parts = decode_file(&bytes).unwrap();
        assert_eq!(parts.salt, salt);
        assert_eq!(parts.params, (M_COST, T_COST, P_COST));
        assert_eq!(parts.nonce, nonce);
        assert_eq!(parts.ciphertext, ct);
    }

    #[test]
    fn decode_rejects_bad_magic_and_short_input() {
        assert!(matches!(decode_file(b"PM"), Err(VaultError::Format(_))));
        let mut bad = encode_file(&[1u8; SALT_LEN], (M_COST, T_COST, P_COST), &[2u8; NONCE_LEN], &[9u8; 64]);
        bad[0] = b'X';
        assert!(matches!(decode_file(&bad), Err(VaultError::Format(_))));
        let mut badver = encode_file(&[1u8; SALT_LEN], (M_COST, T_COST, P_COST), &[2u8; NONCE_LEN], &[9u8; 64]);
        badver[4] = 99;
        assert!(matches!(decode_file(&badver), Err(VaultError::Format(_))));
    }
}

#[cfg(test)]
mod vault_tests {
    use super::*;
    use std::path::Path;

    fn vp(dir: &Path) -> PathBuf {
        dir.join("vault.enc")
    }

    #[test]
    fn create_then_open_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        {
            let mut v = Vault::create(&path, "master-pass-1").unwrap();
            assert!(path.exists());
            v.data.entries.push(Entry::new(
                "Gmail".into(), "me".into(), "pw1".into(),
                "https://gmail.com".into(), "备注".into(), 42,
            ));
            v.data.settings.auto_lock_minutes = 7;
            v.save().unwrap();
        }
        let v2 = Vault::open(&path, "master-pass-1").unwrap();
        assert_eq!(v2.data.entries.len(), 1);
        assert_eq!(v2.data.entries[0].app_name, "Gmail");
        assert_eq!(v2.data.entries[0].notes, "备注");
        assert_eq!(v2.data.settings.auto_lock_minutes, 7);
    }

    #[test]
    fn open_with_wrong_password_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        Vault::create(&path, "right-pass-1").unwrap();
        let err = Vault::open(&path, "wrong-pass-1").unwrap_err();
        assert!(matches!(err, VaultError::WrongPasswordOrCorrupt));
    }

    #[test]
    fn save_creates_bak_with_previous_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        let bak = dir.path().join("vault.enc.bak");
        let mut v = Vault::create(&path, "master-pass-1").unwrap();
        assert!(!bak.exists()); // create 的首次保存时旧文件不存在 → 无 .bak
        v.data.entries.push(Entry::new("A".into(), "".into(), "".into(), "".into(), "".into(), 1));
        v.save().unwrap();
        assert!(bak.exists());
        let old = Vault::open(&bak, "master-pass-1").unwrap();
        assert_eq!(old.data.entries.len(), 0); // .bak 是上一版（空库）
        v.data.entries.push(Entry::new("B".into(), "".into(), "".into(), "".into(), "".into(), 2));
        v.save().unwrap();
        let old2 = Vault::open(&bak, "master-pass-1").unwrap();
        assert_eq!(old2.data.entries.len(), 1); // .bak 滚动为上一版（1 条）
        assert_eq!(old2.data.entries[0].app_name, "A");
    }

    #[test]
    fn tampered_file_fails_to_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        Vault::create(&path, "master-pass-1").unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff; // 篡改 GCM tag 区域
        std::fs::write(&path, &bytes).unwrap();
        let err = Vault::open(&path, "master-pass-1").unwrap_err();
        assert!(matches!(err, VaultError::WrongPasswordOrCorrupt));
    }

    #[test]
    fn change_password_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        let mut v = Vault::create(&path, "old-pass-123").unwrap();
        v.data.entries.push(Entry::new("Bank".into(), "u".into(), "p".into(), "".into(), "".into(), 5));
        v.save().unwrap();
        // 旧密码不对 → 拒绝
        assert!(matches!(
            v.change_password("wrong-old", "new-pass-456"),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
        v.change_password("old-pass-123", "new-pass-456").unwrap();
        drop(v);
        assert!(matches!(Vault::open(&path, "old-pass-123"), Err(VaultError::WrongPasswordOrCorrupt)));
        let v2 = Vault::open(&path, "new-pass-456").unwrap();
        assert_eq!(v2.data.entries.len(), 1);
        assert_eq!(v2.data.entries[0].app_name, "Bank");
    }

    #[test]
    fn export_backup_is_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        Vault::create(&path, "master-pass-1").unwrap();
        let dest = dir.path().join("backup.enc");
        export_backup(&path, &dest).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), std::fs::read(&dest).unwrap());
    }

    #[test]
    fn import_backup_replaces_data_under_current_password() {
        let dir = tempfile::tempdir().unwrap();
        // 当前库：password-A1，条目 AppA
        let path_a = dir.path().join("a.enc");
        let mut va = Vault::create(&path_a, "password-A1").unwrap();
        va.data.entries.push(Entry::new("AppA".into(), "".into(), "".into(), "".into(), "".into(), 1));
        va.save().unwrap();
        // 备份文件：password-B2，条目 AppB
        let path_b = dir.path().join("b.enc");
        {
            let mut vb = Vault::create(&path_b, "password-B2").unwrap();
            vb.data.entries.push(Entry::new("AppB".into(), "".into(), "".into(), "".into(), "".into(), 2));
            vb.save().unwrap();
        }
        // 错误备份密码 → 拒绝，当前数据不变
        assert!(va.import_backup(&path_b, "wrong-pass").is_err());
        assert_eq!(va.data.entries.len(), 1);
        assert_eq!(va.data.entries[0].app_name, "AppA");
        // 正确备份密码 → 替换数据并用当前主密码重加密落盘
        va.import_backup(&path_b, "password-B2").unwrap();
        assert_eq!(va.data.entries.len(), 1);
        assert_eq!(va.data.entries[0].app_name, "AppB");
        drop(va);
        let reopened = Vault::open(&path_a, "password-A1").unwrap();
        assert_eq!(reopened.data.entries[0].app_name, "AppB");
    }
}
