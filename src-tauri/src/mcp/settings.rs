//! MCPサーバーの設定(全プラットフォーム)。既定はすべてオフ・空。
//! エクスポート対象の `UiPrefs` には入れず、`SettingsData.mcp` に置く。

use serde::{Deserialize, Serialize};

/// 既定のポート。衝突しにくい動的ポート範囲(49152〜)の固定値。
pub const DEFAULT_PORT: u16 = 49731;
/// 設定できる最小のポート(特権ポートを避ける)。
pub const MIN_PORT: u16 = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSettings {
    /// 機能全体のトグル。既定はオフ。
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    /// MCP で公開を許可したアカウントの ID。既定は空(どのアカウントも公開しない)。
    #[serde(default)]
    pub exposed_account_ids: Vec<String>,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Default for McpSettings {
    fn default() -> Self {
        Self { enabled: false, port: DEFAULT_PORT, exposed_account_ids: Vec::new() }
    }
}

/// ポートが設定できる範囲か。上限は `u16` の型が保証する。
pub fn validate_port(port: u16) -> Result<(), String> {
    if port < MIN_PORT {
        return Err(format!("ポートは{MIN_PORT}以上を指定してください"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off_with_no_exposed_accounts() {
        let s = McpSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.port, DEFAULT_PORT);
        assert!(s.exposed_account_ids.is_empty());
    }

    #[test]
    fn missing_fields_fall_back_to_the_safe_defaults() {
        let s: McpSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, McpSettings::default());
    }

    #[test]
    fn ports_below_1024_are_rejected() {
        assert!(validate_port(0).is_err());
        assert!(validate_port(1023).is_err());
        assert!(validate_port(1024).is_ok());
        assert!(validate_port(65535).is_ok());
    }
}
