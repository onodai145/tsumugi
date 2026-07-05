//! 複数アカウントの状態管理（Krile の AccountManager 相当）。
//! Phase 1 ではインメモリ保持。SQLite への永続化は store（Phase 6）で差し込む。

use crate::domain::Account;
use crate::error::{Error, Result};

#[derive(Default)]
pub struct AccountManager {
    accounts: Vec<Account>,
    active: Option<String>,
}

impl AccountManager {
    pub fn list(&self) -> Vec<Account> {
        self.accounts.clone()
    }

    #[allow(dead_code)] // Phase 2: 既定アカウントの UI 反映で使用
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn get(&self, account_id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == account_id)
    }

    /// 追加（同一 host + user_id の重複は置き換え）。追加したアカウントを active にする。
    pub fn upsert(&mut self, account: Account) {
        self.active = Some(account.id.clone());
        if let Some(existing) = self
            .accounts
            .iter_mut()
            .find(|a| a.host == account.host && a.user_id == account.user_id)
        {
            *existing = account;
        } else {
            self.accounts.push(account);
        }
    }

    /// 既定アカウントを切り替える。未登録なら [`Error::Invalid`]。
    pub fn set_active(&mut self, account_id: &str) -> Result<()> {
        if self.get(account_id).is_some() {
            self.active = Some(account_id.to_string());
            Ok(())
        } else {
            Err(Error::Invalid(format!("unknown account: {account_id}")))
        }
    }

    /// 削除。active だった場合は先頭アカウントへ寄せる。
    pub fn remove(&mut self, account_id: &str) -> Result<()> {
        let before = self.accounts.len();
        self.accounts.retain(|a| a.id != account_id);
        if self.accounts.len() == before {
            return Err(Error::Invalid(format!("unknown account: {account_id}")));
        }
        if self.active.as_deref() == Some(account_id) {
            self.active = self.accounts.first().map(|a| a.id.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(id: &str, user_id: &str) -> Account {
        Account {
            id: id.into(),
            host: "misskey.io".into(),
            username: "me".into(),
            user_id: user_id.into(),
            display_name: "Me".into(),
            avatar_url: None,
        }
    }

    #[test]
    fn upsert_sets_active_and_dedupes_by_host_user() {
        let mut m = AccountManager::default();
        m.upsert(acc("id1", "u1"));
        assert_eq!(m.active_id(), Some("id1"));
        assert_eq!(m.list().len(), 1);

        // 同一 host+user_id を再登録 → 置き換え（増えない）
        m.upsert(acc("id1b", "u1"));
        assert_eq!(m.list().len(), 1);
        assert_eq!(m.active_id(), Some("id1b"));

        m.upsert(acc("id2", "u2"));
        assert_eq!(m.list().len(), 2);
    }

    #[test]
    fn set_active_rejects_unknown() {
        let mut m = AccountManager::default();
        assert!(matches!(m.set_active("nope"), Err(Error::Invalid(_))));
    }

    #[test]
    fn remove_reassigns_active() {
        let mut m = AccountManager::default();
        m.upsert(acc("id1", "u1"));
        m.upsert(acc("id2", "u2")); // active = id2
        m.remove("id2").unwrap();
        assert_eq!(m.active_id(), Some("id1"));
        assert!(matches!(m.remove("id2"), Err(Error::Invalid(_))));
    }
}
