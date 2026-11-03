//! 設定のエクスポート/インポートの純粋ロジック(I/Oなし)。Issue #544。
//! 設計書: docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md
//! トークンはキーリング管轄でここには現れない。入力ファイルは信頼できない外部データとして扱う。

use crate::domain::{Account, Column, ColumnGroup, MuteConfig, NotifyConfig, PaneNode, UiPrefs};
use crate::error::{Error, Result};
use crate::store::settings::SettingsData;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};

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

/// インポートの計画。`data` を `SettingsStore::replace` に渡す。
#[derive(Debug)]
pub struct ImportPlan {
    pub data: SettingsData,
    pub preview: ImportPreview,
    /// ファイルにだけあり、トークンなしで追加するアカウントのID。
    pub accounts_needing_login: Vec<String>,
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

/// 素のホスト名か。スキーム・パス・ポート・認証情報を含まず、空でないこと。
fn is_bare_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-')
}

/// アカウントの `host` として妥当か。素のホスト名に、任意で `:<1..=65535>` のポートが付いた形だけ通す
/// (ログイン経路がポート付きホストを保存するため)。IPv6リテラル・認証情報・スキーム・パスは通さない。
fn is_valid_account_host(host: &str) -> bool {
    match host.split_once(':') {
        None => is_bare_host(host),
        Some((name, port)) => {
            is_bare_host(name)
                && !port.is_empty()
                && !port.starts_with('0')
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok()
        }
    }
}

fn ensure_unique<'a>(ids: impl Iterator<Item = &'a str>, what: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(invalid(format!("{what}のIDが重複しています")));
        }
    }
    Ok(())
}

fn collect_leaf_group_ids<'a>(node: &'a PaneNode, out: &mut Vec<&'a str>) {
    match node {
        PaneNode::Leaf { group_id, .. } => out.push(group_id),
        PaneNode::Split { children, .. } => {
            for child in children {
                collect_leaf_group_ids(&child.node, out);
            }
        }
    }
}

/// ファイルの内容と現在の設定から、インポート後の設定を組み立てる。ファイルは信頼しない。
/// 1つでも検証に落ちたらファイル全体を拒否する(部分的に取り込まない)。
pub fn plan_import(current: &SettingsData, file: ExportFile) -> Result<ImportPlan> {
    if file.format != EXPORT_FORMAT {
        return Err(invalid("tsumugiの設定ファイルではありません"));
    }
    if file.version != EXPORT_VERSION {
        return Err(invalid(format!("未対応のバージョン(v{})です", file.version)));
    }
    let d = file.data;

    ensure_unique(d.accounts.iter().map(|a| a.id.as_str()), "アカウント")?;
    ensure_unique(d.groups.iter().map(|g| g.id.as_str()), "グループ")?;
    ensure_unique(d.columns.iter().map(|c| c.id.as_str()), "カラム")?;

    let mut identities = HashSet::new();
    for a in &d.accounts {
        if !is_valid_account_host(&a.host) {
            let shown: String = a.host.chars().take(64).collect();
            return Err(invalid(format!("アカウントのホスト名が不正です: {shown}")));
        }
        if !identities.insert((a.host.to_ascii_lowercase(), a.user_id.clone())) {
            return Err(invalid("同じアカウントが複数含まれています"));
        }
    }

    // ファイル側のアカウントID -> 取り込み後のアカウントID
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut accounts = current.accounts.clone();
    let mut accounts_needing_login = Vec::new();
    for fa in &d.accounts {
        let local = current
            .accounts
            .iter()
            .find(|la| la.host.eq_ignore_ascii_case(&fa.host) && la.user_id == fa.user_id);
        match local {
            Some(la) => {
                id_map.insert(fa.id.clone(), la.id.clone());
            }
            None => {
                if current.accounts.iter().any(|la| la.id == fa.id) {
                    return Err(invalid("アカウントのIDがこの端末の別のアカウントと衝突しています"));
                }
                id_map.insert(fa.id.clone(), fa.id.clone());
                // ファイルのURLは信頼しない。アバターやInstance Tickerの画像としてWebViewが
                // 読み込むと、IPアドレスの漏えいや追跡に使われる。再ログインと
                // `refreshInstanceMeta` で正しい値が入る。
                accounts.push(Account { avatar_url: None, instance: None, ..fa.clone() });
                accounts_needing_login.push(fa.id.clone());
            }
        }
    }

    let group_ids: HashSet<&str> = d.groups.iter().map(|g| g.id.as_str()).collect();
    let mut columns = Vec::with_capacity(d.columns.len());
    for c in &d.columns {
        let Some(local_id) = id_map.get(&c.account_id) else {
            return Err(invalid("カラムが存在しないアカウントを参照しています"));
        };
        if !group_ids.contains(c.group_id.as_str()) {
            return Err(invalid("カラムが存在しないグループを参照しています"));
        }
        crate::mml::validate_sound_choice(&c.notify_sound_choice)?;
        let mut c = c.clone();
        c.account_id = local_id.clone();
        columns.push(c);
    }

    if let Some(root) = &d.pane_layout {
        let mut leaves = Vec::new();
        collect_leaf_group_ids(root, &mut leaves);
        if leaves.iter().any(|g| !group_ids.contains(g)) {
            return Err(invalid("ペイン配置が存在しないグループを参照しています"));
        }
    }
    crate::mml::validate_sound_choice(&d.notify.sound_choice)?;

    let mut ui = d.ui;
    ui.background_kind = None;
    ui.background_path = None;
    ui.default_account_id = id_map.get(&ui.default_account_id).cloned().unwrap_or_default();
    ui.note_cache_limit = ui.note_cache_limit.clamp(0, 100_000);
    ui.note_cache_max_age_days = ui.note_cache_max_age_days.clamp(0, 3_650);
    ui.note_cache_max_size_mb = ui.note_cache_max_size_mb.clamp(0, 10_000);
    ui.gap_fill_limit = ui.gap_fill_limit.clamp(0, 1_000);

    let preview = ImportPreview {
        column_count: columns.len() as i32,
        account_count: d.accounts.len() as i32,
        accounts_needing_login: accounts_needing_login.len() as i32,
        has_custom_css: !ui.custom_css.trim().is_empty(),
        has_custom_themes: !ui.custom_themes.is_empty() || !ui.custom_syntax_themes.is_empty(),
        has_custom_endpoints: {
            let defaults = UiPrefs::default();
            ui.search_engine_url != defaults.search_engine_url || ui.summaly_proxy_url != defaults.summaly_proxy_url
        },
    };
    let data = SettingsData {
        accounts,
        groups: d.groups,
        columns,
        mute: d.mute,
        notify: d.notify,
        ui,
        pane_layout: d.pane_layout,
        cache_backend: current.cache_backend.clone(),
        server_mute_snapshots: current.server_mute_snapshots.clone(),
    };
    Ok(ImportPlan { data, preview, accounts_needing_login })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BackgroundKind, ColumnKind, FilterQuery, PaneChild, PaneNode, SplitDirection};

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

    fn current_with_local_account() -> SettingsData {
        let mut d = SettingsData::default();
        d.accounts = vec![account("local1", "misskey.io", "u1")];
        d.groups = vec![group("old-g", 0)];
        d.columns = vec![column("old-c", "local1", "old-g")];
        d
    }

    fn plan_err(current: &SettingsData, file: ExportFile) -> String {
        plan_import(current, file).err().expect("拒否されるはず").to_string()
    }

    #[test]
    fn matching_accounts_keep_the_local_id_and_columns_follow_it() {
        let current = current_with_local_account();
        let mut data = empty_data();
        data.accounts = vec![account("file1", "misskey.io", "u1")];
        data.groups = vec![group("g1", 0)];
        data.columns = vec![column("c1", "file1", "g1")];
        data.ui.default_account_id = "file1".into();

        let plan = plan_import(&current, file_with(data)).unwrap();

        assert_eq!(plan.data.accounts.len(), 1, "端末側のアカウントがそのまま残る");
        assert_eq!(plan.data.accounts[0].id, "local1");
        assert_eq!(plan.data.columns[0].account_id, "local1");
        assert_eq!(plan.data.ui.default_account_id, "local1");
        assert!(plan.accounts_needing_login.is_empty());
        assert_eq!(plan.data.columns.len(), 1, "端末側のカラムはファイルの内容で置き換わる");
        assert_eq!(plan.data.columns[0].id, "c1");
    }

    #[test]
    fn host_comparison_ignores_ascii_case() {
        let current = current_with_local_account();
        let mut data = empty_data();
        data.accounts = vec![account("file1", "Misskey.IO", "u1")];

        let plan = plan_import(&current, file_with(data)).unwrap();

        assert_eq!(plan.data.accounts.len(), 1, "同じアカウントとして対応づく");
        assert!(plan.accounts_needing_login.is_empty());
    }

    #[test]
    fn new_accounts_are_added_without_a_token_and_local_only_accounts_remain() {
        let current = current_with_local_account();
        let mut data = empty_data();
        data.accounts = vec![account("file2", "other.example", "u9")];

        let plan = plan_import(&current, file_with(data)).unwrap();

        let ids: Vec<&str> = plan.data.accounts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["local1", "file2"], "端末側のアカウントは残り、ファイル側が追加される");
        assert_eq!(plan.accounts_needing_login, vec!["file2".to_string()]);
        assert_eq!(plan.preview.accounts_needing_login, 1);
    }

    #[test]
    fn new_accounts_lose_the_external_urls_the_file_supplied() {
        let current = current_with_local_account();
        let mut fa = account("file2", "other.example", "u9");
        fa.avatar_url = Some("https://evil.example/a.png".into());
        fa.instance = Some(crate::domain::InstanceInfo {
            name: Some("Evil".into()),
            icon_url: Some("https://evil.example/i.png".into()),
            theme_color: Some("#ff0000".into()),
        });
        let mut data = empty_data();
        data.accounts = vec![fa];

        let plan = plan_import(&current, file_with(data)).unwrap();

        let stored = plan.data.accounts.iter().find(|a| a.id == "file2").unwrap();
        assert_eq!(stored.avatar_url, None, "ファイル由来のURLは<img>に読み込ませない");
        assert_eq!(stored.instance, None);
        assert_eq!(stored.host, "other.example", "それ以外のフィールドは残る");
    }

    #[test]
    fn a_matched_account_keeps_all_of_the_local_fields() {
        let mut current = current_with_local_account();
        current.accounts[0].display_name = "Local Name".into();
        current.accounts[0].avatar_url = Some("https://misskey.io/local.png".into());
        let local = current.accounts[0].clone();
        let mut fa = account("file1", "misskey.io", "u1");
        fa.display_name = "File Name".into();
        fa.avatar_url = Some("https://evil.example/a.png".into());
        let mut data = empty_data();
        data.accounts = vec![fa];

        let plan = plan_import(&current, file_with(data)).unwrap();

        assert_eq!(plan.data.accounts, vec![local], "端末側のアカウントがそのまま残る");
    }

    #[test]
    fn device_specific_settings_are_carried_over_from_the_current_state() {
        let mut current = current_with_local_account();
        current.server_mute_snapshots.insert("local1".into(), Default::default());

        let plan = plan_import(&current, file_with(empty_data())).unwrap();

        assert!(plan.data.server_mute_snapshots.contains_key("local1"));
    }

    #[test]
    fn background_is_reset_and_the_four_rust_side_numbers_are_clamped() {
        let current = SettingsData::default();
        let mut data = empty_data();
        data.ui.background_kind = Some(BackgroundKind::Video);
        data.ui.background_path = Some("/elsewhere/bg.mp4".into());
        data.ui.note_cache_limit = 9_999_999;
        data.ui.note_cache_max_age_days = -5;
        data.ui.note_cache_max_size_mb = 99_999;
        data.ui.gap_fill_limit = 5_000;
        data.ui.ui_scale = 9_999; // フロントが適用時に丸めるので、ここでは触らない

        let ui = plan_import(&current, file_with(data)).unwrap().data.ui;

        assert_eq!(ui.background_kind, None);
        assert_eq!(ui.background_path, None);
        assert_eq!(ui.note_cache_limit, 100_000);
        assert_eq!(ui.note_cache_max_age_days, 0);
        assert_eq!(ui.note_cache_max_size_mb, 10_000);
        assert_eq!(ui.gap_fill_limit, 1_000);
        assert_eq!(ui.ui_scale, 9_999);
    }

    #[test]
    fn default_account_id_pointing_nowhere_is_cleared() {
        let mut data = empty_data();
        data.ui.default_account_id = "ghost".into();

        let plan = plan_import(&SettingsData::default(), file_with(data)).unwrap();

        assert_eq!(plan.data.ui.default_account_id, "");
    }

    #[test]
    fn preview_counts_and_flags_custom_content() {
        let mut data = empty_data();
        data.accounts = vec![account("f1", "a.example", "u1"), account("f2", "b.example", "u2")];
        data.groups = vec![group("g1", 0)];
        data.columns = vec![column("c1", "f1", "g1"), column("c2", "f2", "g1"), column("c3", "f2", "g1")];
        data.ui.custom_css = "  body{}  ".into();

        let preview = plan_import(&SettingsData::default(), file_with(data)).unwrap().preview;

        assert_eq!(
            preview,
            ImportPreview {
                column_count: 3,
                account_count: 2,
                accounts_needing_login: 2,
                has_custom_css: true,
                has_custom_themes: false,
                has_custom_endpoints: false,
            }
        );
    }

    #[test]
    fn preview_flags_external_service_urls_that_differ_from_the_defaults() {
        let defaults = UiPrefs::default();

        // 既定値のままなら、立たない
        let plan = plan_import(&SettingsData::default(), file_with(empty_data())).unwrap();
        assert!(!plan.preview.has_custom_endpoints);

        // URLプレビューの取得先を変えたら立つ
        let mut data = empty_data();
        data.ui.summaly_proxy_url = "https://evil.example/summaly".into();
        assert!(plan_import(&SettingsData::default(), file_with(data)).unwrap().preview.has_custom_endpoints);

        // 検索エンジンのURLを変えたら立つ
        let mut data = empty_data();
        data.ui.search_engine_url = format!("{}x", defaults.search_engine_url);
        assert!(plan_import(&SettingsData::default(), file_with(data)).unwrap().preview.has_custom_endpoints);
    }

    #[test]
    fn an_empty_data_section_is_planned_as_zero_columns() {
        let preview = plan_import(&current_with_local_account(), file_with(empty_data())).unwrap().preview;

        assert_eq!(preview.column_count, 0);
        assert_eq!(preview.account_count, 0);
    }

    #[test]
    fn rejects_wrong_format_and_unsupported_version() {
        let mut f = file_with(empty_data());
        f.format = "other".into();
        assert!(plan_err(&SettingsData::default(), f).contains("tsumugiの設定ファイルではありません"));

        let mut f = file_with(empty_data());
        f.version = 0;
        assert!(plan_err(&SettingsData::default(), f).contains("未対応のバージョン"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut data = empty_data();
        data.accounts = vec![account("x", "a.example", "u1"), account("x", "b.example", "u2")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("アカウントのIDが重複"));

        let mut data = empty_data();
        data.groups = vec![group("g", 0), group("g", 1)];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("グループのIDが重複"));

        let mut data = empty_data();
        data.accounts = vec![account("a", "a.example", "u1")];
        data.groups = vec![group("g", 0)];
        data.columns = vec![column("c", "a", "g"), column("c", "a", "g")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("カラムのIDが重複"));
    }

    #[test]
    fn rejects_the_same_host_and_user_listed_twice() {
        let mut data = empty_data();
        data.accounts = vec![account("f1", "misskey.io", "u1"), account("f2", "MISSKEY.io", "u1")];

        let err = plan_err(&SettingsData::default(), file_with(data));

        assert!(err.contains("同じアカウントが複数含まれています"), "{err}");
    }

    #[test]
    fn rejects_a_new_account_whose_id_collides_with_another_local_account() {
        let current = current_with_local_account(); // local1 = misskey.io / u1
        let mut data = empty_data();
        data.accounts = vec![account("local1", "other.example", "u9")]; // 同じIDだが別のアカウント

        let err = plan_err(&current, file_with(data));

        assert!(err.contains("この端末の別のアカウントと衝突"), "{err}");
    }

    #[test]
    fn rejects_hosts_that_are_not_bare_host_names() {
        for bad in [
            "https://evil.example",
            "evil.example/path",
            "user@evil.example",
            "",
            ".evil.example",
            "evil..example",
            "evil example",
            // ポートは `host:1..=65535` の形だけ通す
            "evil.example:",
            ":8080",
            "evil.example:0",
            "evil.example:08080",
            "evil.example:65536",
            "evil.example:80:80",
            "evil.example:http",
            "evil.example:+80",
            "evil.example:-80",
            "evil.example: 80",
            "[::1]:80",
            "user@evil.example:80",
            "https://evil.example:80",
        ] {
            let mut data = empty_data();
            data.accounts = vec![account("f1", bad, "u1")];
            let err = plan_err(&SettingsData::default(), file_with(data));
            assert!(err.contains("ホスト名が不正"), "{bad:?}: {err}");
        }
        // 素のホスト名とIPアドレスは通る(通すが、トークンがないので許可リストには入らない)
        for ok in ["misskey.io", "127.0.0.1", "xn--abc.example"] {
            let mut data = empty_data();
            data.accounts = vec![account("f1", ok, "u1")];
            assert!(plan_import(&SettingsData::default(), file_with(data)).is_ok(), "{ok}");
        }
    }

    #[test]
    fn accepts_hosts_with_a_port() {
        for ok in ["misskey.local:8443", "127.0.0.1:3000", "xn--abc.example:443", "misskey.io:65535", "misskey.io:1"] {
            let mut data = empty_data();
            data.accounts = vec![account("f1", ok, "u1")];
            assert!(plan_import(&SettingsData::default(), file_with(data)).is_ok(), "{ok}");
        }
    }

    #[test]
    fn a_port_host_account_matches_the_local_account_with_the_same_host_and_port() {
        let mut current = SettingsData::default();
        current.accounts = vec![account("local1", "misskey.local:8443", "u1")];
        current.groups = vec![group("old-g", 0)];
        let mut data = empty_data();
        data.accounts = vec![account("file1", "MISSKEY.local:8443", "u1")];
        data.groups = vec![group("g1", 0)];
        data.columns = vec![column("c1", "file1", "g1")];

        let plan = plan_import(&current, file_with(data)).unwrap();

        assert_eq!(plan.data.accounts.len(), 1, "新しいアカウントは増えない");
        assert_eq!(plan.data.accounts[0].id, "local1");
        assert!(plan.accounts_needing_login.is_empty());
        assert_eq!(plan.data.columns[0].account_id, "local1");
    }

    #[test]
    fn a_different_port_is_a_different_account() {
        let mut current = SettingsData::default();
        current.accounts = vec![account("local1", "misskey.local:8443", "u1")];
        let mut data = empty_data();
        data.accounts = vec![account("file1", "misskey.local:9000", "u1")];

        let plan = plan_import(&current, file_with(data)).unwrap();

        assert_eq!(plan.accounts_needing_login, vec!["file1".to_string()]);
    }

    #[test]
    fn rejects_dangling_references() {
        let mut data = empty_data();
        data.groups = vec![group("g", 0)];
        data.columns = vec![column("c", "ghost-account", "g")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("存在しないアカウントを参照"));

        let mut data = empty_data();
        data.accounts = vec![account("a", "a.example", "u1")];
        data.columns = vec![column("c", "a", "ghost-group")];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("存在しないグループを参照"));

        let mut data = empty_data();
        data.groups = vec![group("g", 0)];
        data.pane_layout = Some(PaneNode::Split {
            id: "s".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::new_leaf("g"), size: 1.0, auto: false },
                PaneChild { node: PaneNode::new_leaf("ghost-group"), size: 1.0, auto: false },
            ],
        });
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("ペイン配置が存在しないグループを参照"));
    }

    #[test]
    fn rejects_invalid_mml_sound_choices() {
        let mut data = empty_data();
        data.notify.sound_choice = "mml:j".into();
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("MML が不正です"));

        let mut data = empty_data();
        data.accounts = vec![account("a", "a.example", "u1")];
        data.groups = vec![group("g", 0)];
        let mut c = column("c", "a", "g");
        c.notify_sound_choice = "mml:j".into();
        data.columns = vec![c];
        assert!(plan_err(&SettingsData::default(), file_with(data)).contains("MML が不正です"));
    }

    #[test]
    fn a_valid_pane_layout_is_kept_as_is() {
        let mut data = empty_data();
        data.groups = vec![group("g", 0)];
        let layout = PaneNode::new_leaf("g"); // new_leaf はノードIDをランダムに振るので、1つを使い回す
        data.pane_layout = Some(layout.clone());

        let plan = plan_import(&SettingsData::default(), file_with(data)).unwrap();

        assert_eq!(plan.data.pane_layout, Some(layout));
    }

    #[test]
    fn export_then_import_on_the_same_state_changes_nothing_except_the_background() {
        let mut current = current_with_local_account();
        current.ui.custom_css = "a{}".into();
        current.ui.background_path = Some("/x/bg.png".into());
        current.ui.background_kind = Some(BackgroundKind::Image);
        current.ui.default_account_id = "local1".into();
        let file = build_export(&current, "1.0.0", "2026-11-02T00:00:00Z");

        let plan = plan_import(&current, file).unwrap();

        assert_eq!(plan.data.accounts, current.accounts);
        assert_eq!(plan.data.columns, current.columns);
        assert_eq!(plan.data.groups, current.groups);
        assert_eq!(plan.data.ui.custom_css, "a{}");
        assert_eq!(plan.data.ui.default_account_id, "local1");
        assert_eq!(plan.data.ui.background_path, None);
        assert!(plan.accounts_needing_login.is_empty());
    }
}
