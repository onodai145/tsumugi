//! IME の変換候補ウィンドウの位置補正(Issue #502)。
//!
//! Linux/Wayland では、WebKitGTK が IM(fcitx5 等)に渡すキャレット位置が不正確で、変換候補が
//! 入力位置からずれる。素の WebKitGTK アプリ(yad)でも再現するため tsumugi/wry の外の問題だが、
//! `webkit_input_method_context_notify_cursor_area` で座標を上書きすれば候補位置を直せる。
//! フロント(`lib/imeCursorArea.ts`)が求めた実キャレット座標を、ここで IM コンテキストに渡す。
//! WebKit は編集状態が変わるたびに自分の座標を送り直すので、フロントが遅らせて上書きする。

use crate::error::Result;

/// Wayland セッションかどうか。X11 では WebKit 本来の座標が正しい可能性があり未検証なので、
/// Wayland のときだけ上書きする。`GDK_BACKEND` は "wayland,x11" のようなカンマ区切りの優先順で、
/// 先頭が `x11` なら GTK は X11 で動く。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_wayland_session(wayland_display: Option<&str>, gdk_backend: Option<&str>) -> bool {
    let has_display = wayland_display.is_some_and(|v| !v.is_empty());
    let x11_first =
        gdk_backend.is_some_and(|v| v.split(',').next().is_some_and(|b| b.trim() == "x11"));
    has_display && !x11_first
}

/// CSS px を webview のズーム倍率でウィジェット座標(px)に換算する。UI スケール(Issue #40)は
/// `Webview::set_zoom` で実現されており、ズーム時は CSS px とウィジェット座標が食い違う。
/// 不正なズーム(0 以下・非有限)は換算せずそのまま返す。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn scale_by_zoom(value: i32, zoom: f64) -> i32 {
    if !zoom.is_finite() || zoom <= 0.0 {
        return value;
    }
    (f64::from(value) * zoom).round() as i32
}

#[cfg(target_os = "linux")]
fn wayland_session() -> bool {
    use std::sync::OnceLock;
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        is_wayland_session(
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
            std::env::var("GDK_BACKEND").ok().as_deref(),
        )
    })
}

/// IME の変換候補の位置(キャレット矩形)を上書きする。座標は viewport 座標(CSS px)。
/// Linux かつ Wayland セッションのときだけ動き、それ以外は何もしない。
/// 失敗しても入力には影響しないため、呼び出し側(フロント)で握りつぶす想定。
#[tauri::command]
#[specta::specta]
pub fn set_ime_cursor_area(
    window: tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    if wayland_session() {
        window
            .with_webview(move |w| {
                use webkit2gtk::{InputMethodContextExt, WebViewExt};
                let webview = w.inner();
                if let Some(input_context) = webview.input_method_context() {
                    let zoom = webview.zoom_level();
                    input_context.notify_cursor_area(
                        scale_by_zoom(x, zoom),
                        scale_by_zoom(y, zoom),
                        scale_by_zoom(width, zoom),
                        scale_by_zoom(height, zoom),
                    );
                }
            })
            .map_err(|e| crate::error::Error::Invalid(e.to_string()))?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (window, x, y, width, height);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_when_display_set_and_backend_unset() {
        assert!(is_wayland_session(Some("wayland-1"), None));
    }

    #[test]
    fn not_wayland_when_display_unset() {
        assert!(!is_wayland_session(None, None));
    }

    #[test]
    fn not_wayland_when_display_empty() {
        assert!(!is_wayland_session(Some(""), None));
    }

    #[test]
    fn not_wayland_when_backend_forced_x11() {
        assert!(!is_wayland_session(Some("wayland-1"), Some("x11")));
    }

    #[test]
    fn not_wayland_when_x11_is_first_in_backend_list() {
        assert!(!is_wayland_session(Some("wayland-1"), Some("x11,wayland")));
    }

    #[test]
    fn wayland_when_wayland_is_first_in_backend_list() {
        assert!(is_wayland_session(Some("wayland-1"), Some("wayland,x11")));
    }

    #[test]
    fn scale_rounds_to_nearest() {
        assert_eq!(scale_by_zoom(100, 1.0), 100);
        assert_eq!(scale_by_zoom(100, 1.25), 125);
        assert_eq!(scale_by_zoom(10, 0.5), 5);
        assert_eq!(scale_by_zoom(3, 1.5), 5);
    }

    #[test]
    fn scale_keeps_negative_values() {
        assert_eq!(scale_by_zoom(-10, 2.0), -20);
    }

    #[test]
    fn scale_ignores_invalid_zoom() {
        assert_eq!(scale_by_zoom(10, 0.0), 10);
        assert_eq!(scale_by_zoom(10, -1.0), 10);
        assert_eq!(scale_by_zoom(10, f64::NAN), 10);
        assert_eq!(scale_by_zoom(10, f64::INFINITY), 10);
    }
}
