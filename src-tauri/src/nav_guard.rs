//! Android でメイン WebView が外部オリジンへ遷移するのを止める(Issue #660)。
//!
//! Android の WebView は `target="_blank"` の新規ウィンドウに対応しておらず、`<a target="_blank">` や
//! `window.open()` がメイン WebView 自身の遷移になる。外部ページが Tauri の IPC スクリプトを注入された
//! ままメインウィンドウに載り、戻るとアプリが再読み込みされて状態も失われる。フロントエンドの
//! `lib/externalLinks.ts`(Issue #657)が普通のリンクは先に OS 側へ渡すが、ここは取りこぼした経路の
//! 多層防御として、Rust 側でメインフレームの遷移を判定する。
//!
//! Android だけで登録する。デスクトップは新規ウィンドウの要求を別経路で扱うので同じ問題が無く、
//! wry の遷移フックはフレームの区別なく(Linux の WebKitGTK・macOS はサブフレームにも)呼ばれるため、
//! 外部オリジンを一律に止めると URL プレビューの再生用 `<iframe>` が壊れる。Android は実機で、メインフレーム
//! の遷移では呼ばれ、`<iframe>` の読み込みでは呼ばれないことを確認している。
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use tauri::plugin::TauriPlugin;
use tauri::{Manager, Runtime, Url};
use tauri_plugin_opener::OpenerExt;

/// Android の本番でアプリのページが載るホスト(Tauri が WebView 内で解決する擬似ホスト)。
const APP_HOST: &str = "tauri.localhost";

/// 遷移の扱い。
#[derive(Debug, PartialEq, Eq)]
pub enum NavDecision {
    /// そのまま遷移させる(アプリ自身のページ)。
    Allow,
    /// 遷移を止めて、OS 側(既定のブラウザ等)で開く。
    OpenExternal,
    /// 遷移を止める。WebView で読み込んではいけないスキーム。
    Block,
}

/// `url` への遷移をどう扱うか。`dev_url` は `cargo tauri dev` のときだけ渡す開発サーバーの URL。
pub fn decide(url: &Url, dev_url: Option<&Url>) -> NavDecision {
    if url.as_str() == "about:blank" {
        return NavDecision::Allow;
    }
    match url.scheme() {
        "http" | "https" => {
            if url.host_str() == Some(APP_HOST) && url.port().is_none() {
                return NavDecision::Allow;
            }
            if dev_url.is_some_and(|dev| url.origin() == dev.origin()) {
                return NavDecision::Allow;
            }
            NavDecision::OpenExternal
        }
        "mailto" | "tel" => NavDecision::OpenExternal,
        _ => NavDecision::Block,
    }
}

/// メイン WebView の遷移を `decide` で判定するプラグイン。
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("nav-guard")
        .on_navigation(|webview, url| {
            let app = webview.app_handle();
            let dev_url = if tauri::is_dev() {
                app.config().build.dev_url.clone()
            } else {
                None
            };
            match decide(url, dev_url.as_ref()) {
                NavDecision::Allow => true,
                NavDecision::OpenExternal => {
                    // この遷移フックは Android の UI スレッドで呼ばれる。モバイルの opener は Kotlin 側の応答を
                    // 同期で待つので、ここで直接呼ぶと UI スレッドが自分自身の応答を待って固まる(実機で ANR になった)。
                    // 開く処理は別スレッドへ逃がし、フックはすぐ `false`(遷移をキャンセル)を返す。
                    let app = app.clone();
                    let url = url.clone();
                    std::thread::spawn(move || {
                        // URL にはトークンなどが含まれうるので、ログにはスキームとホストだけを出す。
                        if let Err(e) = app.opener().open_url(url.as_str(), None::<&str>) {
                            log::warn!(
                                "nav_guard: failed to open a {} URL (host {:?}) externally: {e}",
                                url.scheme(),
                                url.host_str()
                            );
                        }
                    });
                    false
                }
                NavDecision::Block => {
                    log::warn!("nav_guard: blocked a navigation to a {} URL", url.scheme());
                    false
                }
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Url;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap_or_else(|e| panic!("test url {s:?} must parse: {e}"))
    }

    #[test]
    fn allows_the_apps_own_origin() {
        for s in [
            "http://tauri.localhost/",
            "http://tauri.localhost/index.html",
            "http://tauri.localhost/#/profile?x=1",
            "https://tauri.localhost/",
            "about:blank",
        ] {
            assert_eq!(decide(&url(s), None), NavDecision::Allow, "{s}");
        }
    }

    #[test]
    fn hands_external_http_and_mail_links_to_the_os() {
        for s in [
            "https://example.com/",
            "http://example.com/a?b=1",
            "mailto:someone@example.com",
            "tel:+810000000000",
        ] {
            assert_eq!(decide(&url(s), None), NavDecision::OpenExternal, "{s}");
        }
    }

    #[test]
    fn does_not_mistake_lookalike_hosts_for_the_app() {
        for s in [
            "http://tauri.localhost.evil.example/",
            "http://sub.tauri.localhost/",
            "http://tauri.localhost:8080/",
            "http://evil.example/?u=http://tauri.localhost/",
        ] {
            assert_eq!(decide(&url(s), None), NavDecision::OpenExternal, "{s}");
        }
    }

    #[test]
    fn blocks_schemes_that_must_never_load_in_the_webview() {
        for s in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
            "intent://scan/#Intent;scheme=zxing;end",
            "blob:http://tauri.localhost/00000000-0000-0000-0000-000000000000",
            "ftp://example.com/",
        ] {
            assert_eq!(decide(&url(s), None), NavDecision::Block, "{s}");
        }
    }

    #[test]
    fn the_dev_server_is_the_app_only_when_one_is_given() {
        let dev = url("http://127.0.0.1:5173");
        assert_eq!(
            decide(&url("http://127.0.0.1:5173/"), Some(&dev)),
            NavDecision::Allow
        );
        assert_eq!(
            decide(&url("http://127.0.0.1:5173/src/main.ts"), Some(&dev)),
            NavDecision::Allow
        );
        // 別のポート・別のホストは、dev サーバーがあっても外部。
        assert_eq!(
            decide(&url("http://127.0.0.1:5174/"), Some(&dev)),
            NavDecision::OpenExternal
        );
        assert_eq!(
            decide(&url("http://localhost:5173/"), Some(&dev)),
            NavDecision::OpenExternal
        );
        // dev サーバーが無い(本番)ときは、同じ URL も外部。
        assert_eq!(
            decide(&url("http://127.0.0.1:5173/"), None),
            NavDecision::OpenExternal
        );
    }
}
