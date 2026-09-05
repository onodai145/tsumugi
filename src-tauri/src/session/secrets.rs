//! アクセストークンのセキュアストア。**token は Core 内でのみ扱い、フロントへ渡さない**。
//! 実行時は OS の Secret 機構（keyring-core）を、テストではインメモリ実装を使う。
//!
//! keyring crate の v1 互換ラッパ(`keyring::Entry`)は Android 未対応のため使わず、
//! keyring-core + プラットフォームごとの store crate を直接使う
//! (macOS: apple-native-keyring-store / Windows: windows-native-keyring-store /
//!  Linux 等 unix: zbus-secret-service-keyring-store / Android: android-native-keyring-store)。
//! Android 版は Tauri Mobile が ndk-context を自動初期化してくれないため、
//! `MainActivity.onCreate` から JNI 経由で `Keyring.initializeNdkContext(applicationContext)`
//! を明示的に呼んでいる(gen/android/.../MainActivity.kt 参照)。

use crate::error::{Error, Result};
use std::sync::Once;

const SERVICE: &str = "com.onodai.tsumugi";

/// account_id をキーに token を出し入れするストア。
pub trait SecretStore: Send + Sync {
    fn set(&self, account_id: &str, token: &str) -> Result<()>;
    fn get(&self, account_id: &str) -> Result<Option<String>>;
    fn delete(&self, account_id: &str) -> Result<()>;
}

static INIT_STORE: Once = Once::new();

/// プラットフォーム別のデフォルトストアを一度だけ登録する。
fn ensure_default_store() -> Result<()> {
    let mut init_err: Option<String> = None;
    INIT_STORE.call_once(|| {
        if let Err(e) = init_default_store() {
            init_err = Some(e.to_string());
        }
    });
    match init_err {
        Some(e) => Err(Error::Secret(e)),
        None => Ok(()),
    }
}

fn init_default_store() -> std::result::Result<(), keyring_core::Error> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new()?;
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new()?;
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    let store = zbus_secret_service_keyring_store::Store::new()?;
    #[cfg(target_os = "android")]
    let store = android_native_keyring_store::Store::new()?;

    keyring_core::set_default_store(store);
    Ok(())
}

/// OS 標準の Secret 機構（Windows Credential Manager / macOS Keychain / Linux Secret Service /
/// Android Keystore + SharedPreferences）。
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account_id: &str) -> Result<keyring_core::Entry> {
        ensure_default_store()?;
        keyring_core::Entry::new(SERVICE, account_id).map_err(|e| Error::Secret(e.to_string()))
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
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Secret(e.to_string())),
        }
    }

    fn delete(&self, account_id: &str) -> Result<()> {
        match Self::entry(account_id)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Secret(e.to_string())),
        }
    }
}

/// cache backend(Postgres接続)のパスワード保存用サービス名。account トークン(`SERVICE`)とは
/// 別の固定文字列にして、同じユーザー名(こちらは常に単一の固定キー)が衝突しないようにする。
const CACHE_BACKEND_SERVICE: &str = "com.onodai.tsumugi-cache-backend";
/// cache backendは(現状)アカウント単位ではなくアプリ全体で1つの接続情報を持つため、
/// エントリのユーザー名部分は固定文字列にする。
const CACHE_BACKEND_ENTRY_USER: &str = "postgres";

fn cache_backend_entry() -> Result<keyring_core::Entry> {
    ensure_default_store()?;
    keyring_core::Entry::new(CACHE_BACKEND_SERVICE, CACHE_BACKEND_ENTRY_USER)
        .map_err(|e| Error::Secret(e.to_string()))
}

/// cache backend(Postgres)のパスワードをOS keyringへ保存する(Issue #115 Phase 2)。
pub(crate) fn save_cache_backend_password(password: &str) -> Result<()> {
    cache_backend_entry()?
        .set_password(password)
        .map_err(|e| Error::Secret(e.to_string()))
}

/// cache backend(Postgres)のパスワードをOS keyringから読み出す(Issue #115 Phase 2)。
/// 未保存なら`Ok(None)`。
pub(crate) fn load_cache_backend_password() -> Result<Option<String>> {
    match cache_backend_entry()?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

/// cache backend(Postgres)のパスワードをOS keyringから削除する(Issue #115 Phase 2)。
/// 未保存でもエラーにしない。現状どのコマンドからも呼ばれていないが(SettingsStore側は
/// Sqliteへ切り替えてもパスワードのkeyring削除まではしない設計)、keyring読み書きの
/// 一貫した3操作セットとして用意しておく。
#[allow(dead_code)]
pub(crate) fn delete_cache_backend_password() -> Result<()> {
    match cache_backend_entry()?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Secret(e.to_string())),
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

#[cfg(test)]
mod real_keyring_tests {
    use super::*;

    /// 実 OS シークレットストアへの疎通テスト。CI 等ヘッドレス環境では実行できないため #[ignore]。
    #[test]
    #[ignore]
    fn keyring_store_roundtrip_real_os_backend() {
        let s = KeyringStore;
        let id = format!("tsumugi-keyring-test-{}", std::process::id());
        assert!(s.get(&id).unwrap().is_none());
        s.set(&id, "tok-123").unwrap();
        assert_eq!(s.get(&id).unwrap().as_deref(), Some("tok-123"));
        s.delete(&id).unwrap();
        assert!(s.get(&id).unwrap().is_none());
    }

    /// cache backendパスワードの実 OS シークレットストアへの疎通テスト。CI 等ヘッドレス環境
    /// では実行できないため #[ignore]（keyring_store_roundtrip_real_os_backend と同じ方針）。
    #[test]
    #[ignore]
    fn cache_backend_password_roundtrip_real_os_backend() {
        delete_cache_backend_password().unwrap();
        assert!(load_cache_backend_password().unwrap().is_none());
        save_cache_backend_password("pg-pass-123").unwrap();
        assert_eq!(load_cache_backend_password().unwrap().as_deref(), Some("pg-pass-123"));
        delete_cache_backend_password().unwrap();
        assert!(load_cache_backend_password().unwrap().is_none());
    }
}
