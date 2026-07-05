//! 認証・アカウント管理。token は本層より外（フロント）へ出さない。

pub mod account_manager;
pub mod miauth;
pub mod secrets;

pub use account_manager::AccountManager;
// MemoryStore はテスト・ヘッドレス用（Phase 2 の統合テストで使用）。
#[allow(unused_imports)]
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
