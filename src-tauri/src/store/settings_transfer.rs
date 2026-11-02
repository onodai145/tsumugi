//! 設定のエクスポート/インポートの純粋ロジック(I/Oなし)。Issue #544。
//! 設計書: docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md
//! トークンはキーリング管轄でここには現れない。入力ファイルは信頼できない外部データとして扱う。

use crate::domain::{Account, Column, ColumnGroup, MuteConfig, NotifyConfig, PaneNode, UiPrefs};
use crate::error::{Error, Result};
use crate::store::settings::SettingsData;
use serde::{Deserialize, Serialize};
use specta::Type;

pub const EXPORT_FORMAT: &str = "tsumugi-settings";
pub const EXPORT_VERSION: u32 = 1;
/// 読み込むファイルのサイズ上限。カスタムCSS(256KiB)より十分大きい値にしてある。
pub const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub app_version: String,
    pub data: ExportData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportData {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub groups: Vec<ColumnGroup>,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub mute: MuteConfig,
    #[serde(default)]
    pub notify: NotifyConfig,
    #[serde(default)]
    pub ui: UiPrefs,
    #[serde(default)]
    pub pane_layout: Option<PaneNode>,
}

/// インポート前に利用者へ見せる概要。
#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub column_count: i32,
    pub account_count: i32,
    /// ファイルにだけあり、トークンなしで追加されるアカウントの数。
    pub accounts_needing_login: i32,
    pub has_custom_css: bool,
    pub has_custom_themes: bool,
    /// 検索エンジンまたはURLプレビュー(summaly)のURLが、既定値と異なる。共有ファイルで、
    /// 送信先を他人のサーバーに向けられるため、確認ダイアログで知らせる。
    pub has_custom_endpoints: bool,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub column_count: i32,
    /// トークンなしで追加したアカウントのID。再認証が必要。
    pub accounts_needing_login: Vec<String>,
}

/// 書き出す内容を組み立てる。端末固有の項目(`cache_backend`、同期スナップショット、背景のパス)は含めない。
pub fn build_export(data: &SettingsData, app_version: &str, exported_at: &str) -> ExportFile {
    let mut ui = data.ui.clone();
    ui.background_kind = None;
    ui.background_path = None;
    ExportFile {
        format: EXPORT_FORMAT.into(),
        version: EXPORT_VERSION,
        exported_at: exported_at.into(),
        app_version: app_version.into(),
        data: ExportData {
            accounts: data.accounts.clone(),
            groups: data.groups.clone(),
            columns: data.columns.clone(),
            mute: data.mute.clone(),
            notify: data.notify.clone(),
            ui,
            pane_layout: data.pane_layout.clone(),
        },
    }
}

/// ファイルの中身を読む。サイズ・形式・バージョンを、本体を読む前に確認する。
pub fn parse_export(bytes: &[u8]) -> Result<ExportFile> {
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(Error::Invalid("ファイルが大きすぎます(上限8MiB)".into()));
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes); // UTF-8 BOM
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| Error::Invalid(format!("JSONとして読み込めません: {e}")))?;
    if value.get("format").and_then(|v| v.as_str()) != Some(EXPORT_FORMAT) {
        return Err(Error::Invalid("tsumugiの設定ファイルではありません".into()));
    }
    if let Some(v) = value.get("version").and_then(|v| v.as_u64()) {
        if v > u64::from(EXPORT_VERSION) {
            return Err(Error::Invalid(format!("より新しいバージョン(v{v})で書き出されたファイルです")));
        }
    }
    serde_json::from_value(value).map_err(|e| Error::Invalid(format!("設定ファイルの内容が不正です: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BackgroundKind, ColumnKind, FilterQuery};

    pub(super) fn account(id: &str, host: &str, user_id: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
            username: format!("name-{user_id}"),
            user_id: user_id.into(),
            display_name: "Me".into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    pub(super) fn group(id: &str, order: i32) -> ColumnGroup {
        ColumnGroup { id: id.into(), order, width: 300, auto: false }
    }

    pub(super) fn column(id: &str, account_id: &str, group_id: &str) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind: ColumnKind::Home,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: group_id.into(),
            title: None,
        }
    }

    pub(super) fn empty_data() -> ExportData {
        ExportData {
            accounts: vec![],
            groups: vec![],
            columns: vec![],
            mute: MuteConfig::default(),
            notify: NotifyConfig::default(),
            ui: UiPrefs::default(),
            pane_layout: None,
        }
    }

    pub(super) fn file_with(data: ExportData) -> ExportFile {
        ExportFile {
            format: EXPORT_FORMAT.into(),
            version: EXPORT_VERSION,
            exported_at: "2026-11-02T00:00:00Z".into(),
            app_version: "0.0.0".into(),
            data,
        }
    }

    fn settings_data() -> SettingsData {
        let mut d = SettingsData::default();
        d.accounts = vec![account("a1", "misskey.io", "u1")];
        d.groups = vec![group("g1", 0)];
        d.columns = vec![column("c1", "a1", "g1")];
        d.ui.background_kind = Some(BackgroundKind::Image);
        d.ui.background_path = Some("/home/me/bg.png".into());
        d.ui.custom_css = "body { color: red }".into();
        d.cache_backend = crate::domain::CacheBackendConfig::default();
        d
    }

    #[test]
    fn build_export_drops_background_paths_and_device_specific_data() {
        let mut d = settings_data();
        d.server_mute_snapshots.insert("a1".into(), Default::default());

        let file = build_export(&d, "1.2.3", "2026-11-02T00:00:00Z");

        assert_eq!(file.format, EXPORT_FORMAT);
        assert_eq!(file.version, EXPORT_VERSION);
        assert_eq!(file.app_version, "1.2.3");
        assert_eq!(file.exported_at, "2026-11-02T00:00:00Z");
        assert_eq!(file.data.ui.background_kind, None);
        assert_eq!(file.data.ui.background_path, None);
        assert_eq!(file.data.ui.custom_css, "body { color: red }", "それ以外のUI設定は残る");
        assert_eq!(file.data.accounts, d.accounts);
        let json = serde_json::to_string(&file).unwrap();
        assert!(!json.contains("cacheBackend"), "キャッシュDBの接続設定は含めない");
        assert!(!json.contains("serverMuteSnapshots"), "同期スナップショットは含めない");
        assert!(!json.contains("/home/me/bg.png"), "端末ローカルのパスは含めない");
    }

    #[test]
    fn parse_export_reads_what_build_export_wrote() {
        let file = build_export(&settings_data(), "1.2.3", "2026-11-02T00:00:00Z");
        let bytes = serde_json::to_vec_pretty(&file).unwrap();

        assert_eq!(parse_export(&bytes).unwrap(), file);
    }

    #[test]
    fn parse_export_accepts_a_utf8_bom() {
        let file = build_export(&settings_data(), "1.2.3", "2026-11-02T00:00:00Z");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(serde_json::to_vec(&file).unwrap());

        assert_eq!(parse_export(&bytes).unwrap(), file);
    }

    #[test]
    fn parse_export_accepts_an_empty_data_section() {
        let bytes = br#"{"format":"tsumugi-settings","version":1,"exportedAt":"x","appVersion":"y","data":{}}"#;

        let file = parse_export(bytes).unwrap();

        assert!(file.data.accounts.is_empty());
        assert!(file.data.columns.is_empty());
        assert_eq!(file.data.ui, UiPrefs::default());
    }

    #[test]
    fn parse_export_rejects_files_that_are_not_tsumugi_settings() {
        let err = parse_export(br#"{"hello":"world"}"#).unwrap_err().to_string();
        assert!(err.contains("tsumugiの設定ファイルではありません"), "{err}");

        let err = parse_export(br#"{"format":"other","version":1,"data":{}}"#).unwrap_err().to_string();
        assert!(err.contains("tsumugiの設定ファイルではありません"), "{err}");
    }

    #[test]
    fn parse_export_rejects_newer_versions_before_reading_the_body() {
        // 将来の形式は本体の構造が違うかもしれないので、本体を読む前にバージョンで弾く。
        let bytes = br#"{"format":"tsumugi-settings","version":2,"data":"not an object"}"#;

        let err = parse_export(bytes).unwrap_err().to_string();

        assert!(err.contains("より新しいバージョン(v2)"), "{err}");
    }

    #[test]
    fn parse_export_rejects_invalid_json_and_oversized_files() {
        let err = parse_export(b"{not json").unwrap_err().to_string();
        assert!(err.contains("JSONとして読み込めません"), "{err}");

        let big = vec![b' '; MAX_IMPORT_BYTES + 1];
        let err = parse_export(&big).unwrap_err().to_string();
        assert!(err.contains("ファイルが大きすぎます"), "{err}");
    }
}
