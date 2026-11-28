//! `tauri.conf.json` の CSP が緩んでいないことを確かめる(Issue #612、
//! 設計は `docs/superpowers/specs/2026-11-28-issue-612-csp-design.md`)。

use serde_json::Value;

fn security() -> Value {
    let conf: Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
    conf["app"]["security"].clone()
}

/// ディレクティブの値(文字列でも配列でもよい)を、空白区切りのトークンにする。
fn tokens(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => s.split_whitespace().map(str::to_owned).collect(),
        Value::Array(a) => a.iter().flat_map(tokens).collect(),
        other => panic!("CSP の値が文字列でも配列でもありません: {other}"),
    }
}

fn directives() -> Vec<(String, Vec<String>)> {
    let security = security();
    let csp = security["csp"]
        .as_object()
        .expect("csp が設定されていません");
    csp.iter().map(|(k, v)| (k.clone(), tokens(v))).collect()
}

fn directive(name: &str) -> Vec<String> {
    directives()
        .into_iter()
        .find(|(k, _)| k == name)
        .unwrap_or_else(|| panic!("csp.{name} がありません"))
        .1
}

#[test]
fn script_and_default_sources_are_self_only() {
    assert_eq!(directive("script-src"), ["'self'"]);
    assert_eq!(directive("default-src"), ["'self'"]);
}

/// `script-src-elem` / `script-src-attr` は `script-src` より優先されるので、置かせない。
#[test]
fn script_src_is_not_overridden() {
    for (name, _) in directives() {
        assert!(
            name != "script-src-elem" && name != "script-src-attr",
            "{name} は script-src を上書きします"
        );
    }
}

#[test]
fn eval_is_never_allowed() {
    for (name, values) in directives() {
        for value in values {
            assert!(
                value != "'unsafe-eval'" && value != "'wasm-unsafe-eval'",
                "{name} に {value} があります"
            );
        }
    }
}

#[test]
fn unsafe_inline_is_only_for_styles() {
    for (name, values) in directives() {
        if values.iter().any(|v| v == "'unsafe-inline'") {
            assert_eq!(name, "style-src");
        }
    }
}

#[test]
fn unused_directives_are_closed() {
    for name in ["object-src", "base-uri", "form-action", "frame-ancestors"] {
        assert_eq!(directive(name), ["'none'"], "{name}");
    }
}

/// `http://ipc.localhost` のような特定の接続先はよいが、スキーム全体は許可しない。
#[test]
fn connect_src_has_no_wildcard() {
    for value in directive("connect-src") {
        assert!(
            !["*", "http:", "https:", "ws:", "wss:"].contains(&value.as_str()),
            "connect-src に {value} があります"
        );
    }
}

#[test]
fn only_style_src_modification_is_disabled() {
    assert_eq!(
        security()["dangerousDisableAssetCspModification"],
        serde_json::json!(["style-src"])
    );
}

/// Tauri は `tauri.<platform>.conf.json` を本体の設定に重ねるので、どれも `app.security` を持たせない。
#[test]
fn platform_configs_do_not_override_security() {
    let mut checked = 0;
    for entry in std::fs::read_dir(env!("CARGO_MANIFEST_DIR")).expect("src-tauri") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !(name.starts_with("tauri.") && name.ends_with(".conf.json"))
            || name == "tauri.conf.json"
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read config");
        let conf: Value = serde_json::from_str(&text).expect("parse config");
        assert!(
            conf["app"]["security"].is_null(),
            "{name} が app.security を上書きします"
        );
        checked += 1;
    }
    assert!(checked >= 1, "tauri.android.conf.json が見つかりません");
}
