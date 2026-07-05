//! アクセストークンのセキュアストア。**token は Core 内でのみ扱い、フロントへ渡さない**。
//! 実行時は OS の Secret 機構（keyring）を、テストではインメモリ実装を使う。

use crate::error::{Error, Result};

const SERVICE: &str = "com.onodai.tsumugi";

/// account_id をキーに token を出し入れするストア。
pub trait SecretStore: Send + Sync {
    fn set(&self, account_id: &str, token: &str) -> Result<()>;
    fn get(&self, account_id: &str) -> Result<Option<String>>;
    fn delete(&self, account_id: &str) -> Result<()>;
}

/// OS 標準の Secret 機構（Windows Credential Manager / macOS Keychain / Linux Secret Service）。
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account_id: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, account_id).map_err(|e| Error::Secret(e.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, account_id: &str, token: &str) -> Result<()> {
        Self::entry(account_id)?
            .set_password(token)
            .map_err(|e| Error::Secret(e.to_string()))
    }

    fn get(&self, account_id: &str) -> Result<Option<String>> {
        match Self::entry(account_id)?.get_password() {
            Ok(t) => Ok(Some(t)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Secret(e.to_string())),
        }
    }

    fn delete(&self, account_id: &str) -> Result<()> {
        match Self::entry(account_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Secret(e.to_string())),
        }
    }
}

/// テスト・ヘッドレス環境用のインメモリ実装。
#[derive(Default)]
#[allow(dead_code)]
pub struct MemoryStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn set(&self, account_id: &str, token: &str) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .insert(account_id.to_string(), token.to_string());
        Ok(())
    }

    fn get(&self, account_id: &str) -> Result<Option<String>> {
        Ok(self.inner.lock().unwrap().get(account_id).cloned())
    }

    fn delete(&self, account_id: &str) -> Result<()> {
        self.inner.lock().unwrap().remove(account_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip() {
        let s = MemoryStore::default();
        assert!(s.get("acc1").unwrap().is_none());
        s.set("acc1", "tok").unwrap();
        assert_eq!(s.get("acc1").unwrap().as_deref(), Some("tok"));
        s.delete("acc1").unwrap();
        assert!(s.get("acc1").unwrap().is_none());
        // 二重 delete はエラーにしない
        s.delete("acc1").unwrap();
    }
}
