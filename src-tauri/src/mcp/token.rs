//! MCPサーバーの bearer トークン(Issue #557)。keyring に保存し、実行中のサーバーとは
//! `SharedToken` で共有する。トークンはログに出さない。

use crate::error::{Error, Result};
use crate::session::SecretStore;
use base64::Engine;
use std::sync::{Arc, RwLock};

/// `SecretStore` のキー。アカウントIDはUUIDなので衝突しない。
pub const SECRET_KEY: &str = "__mcp_token__";

/// OSの乱数による256bitのトークン(base64url、パディングなし)。
pub fn generate() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::Secret(format!("failed to generate the MCP token: {e}")))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// 保存済みのトークンを返す。無ければ作って保存する。keyring が使えなければエラー。
pub fn load_or_create(store: &dyn SecretStore) -> Result<String> {
    if let Some(t) = store.get(SECRET_KEY)? {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let t = generate()?;
    store.set(SECRET_KEY, &t)?;
    Ok(t)
}

/// 新しいトークンに差し替える(古いものは使えなくなる)。
pub fn regenerate(store: &dyn SecretStore) -> Result<String> {
    let t = generate()?;
    store.set(SECRET_KEY, &t)?;
    Ok(t)
}

/// 実行中のサーバーが、リクエストごとに現在のトークンを読むための共有値。
#[derive(Clone, Default)]
pub struct SharedToken(Arc<RwLock<Option<String>>>);

impl SharedToken {
    pub fn set(&self, token: String) {
        *self.0.write().unwrap() = Some(token);
    }
    #[cfg(test)]
    pub fn clear(&self) {
        *self.0.write().unwrap() = None;
    }
    pub fn get(&self) -> Option<String> {
        self.0.read().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MemoryStore;

    /// どの操作も失敗する `SecretStore`(keyring が使えない環境の再現)。
    struct FailingStore;
    impl SecretStore for FailingStore {
        fn set(&self, _: &str, _: &str) -> Result<()> {
            Err(Error::Secret("keyring unavailable".into()))
        }
        fn get(&self, _: &str) -> Result<Option<String>> {
            Err(Error::Secret("keyring unavailable".into()))
        }
        fn delete(&self, _: &str) -> Result<()> {
            Err(Error::Secret("keyring unavailable".into()))
        }
    }

    #[test]
    fn generate_is_43_url_safe_chars_and_differs_each_time() {
        let a = generate().unwrap();
        let b = generate().unwrap();
        assert_eq!(a.len(), 43, "32バイトのbase64url(パディングなし)");
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a, b);
    }

    #[test]
    fn load_or_create_creates_once_and_then_returns_the_same_token() {
        let store = MemoryStore::default();
        let first = load_or_create(&store).unwrap();
        let second = load_or_create(&store).unwrap();
        assert_eq!(first, second);
        assert_eq!(store.get(SECRET_KEY).unwrap().as_deref(), Some(first.as_str()));
    }

    #[test]
    fn regenerate_replaces_the_stored_token() {
        let store = MemoryStore::default();
        let old = load_or_create(&store).unwrap();
        let new = regenerate(&store).unwrap();
        assert_ne!(old, new);
        assert_eq!(load_or_create(&store).unwrap(), new);
    }

    #[test]
    fn load_or_create_fails_when_the_secret_store_fails() {
        // keyring が使えないときは、平文などへフォールバックせずエラーにする。
        assert!(load_or_create(&FailingStore).is_err());
        assert!(regenerate(&FailingStore).is_err());
    }

    #[test]
    fn shared_token_set_get_clear() {
        let t = SharedToken::default();
        assert_eq!(t.get(), None);
        t.set("abc".into());
        assert_eq!(t.get().as_deref(), Some("abc"));
        let clone = t.clone();
        t.set("def".into());
        assert_eq!(clone.get().as_deref(), Some("def"), "クローンは同じ値を共有する");
        t.clear();
        assert_eq!(clone.get(), None);
    }
}
