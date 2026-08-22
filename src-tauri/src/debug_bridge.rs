//! デバッグビルド限定: Claude等がユーザーの実行中インスタンスに対して任意のJSを実行し、
//! DOM調査・`console.log`確認を行うためのUnixドメインソケットブリッジ(Issue #232)。
//!
//! TCP(127.0.0.1:PORT)ではなくUnixドメインソケットを採用しているのは、ブラウザの
//! `fetch()`から到達できてしまう(localhost drive-by/DNS rebinding系の攻撃パターン)
//! TCPの弱点を、Unixドメインソケットなら原理的に排除できるため。
//! 詳細: docs/superpowers/specs/2026-08-22-claude-devtools-bridge-design.md
//!
//! プロトコルはHTTPのごく最小限のサブセット(リクエストボディ=実行するJS文字列、
//! `Content-Length`のみ解釈、メソッド/パスは無視)。`curl --unix-socket <path> -d '<js>'
//! http://localhost/` のような形で叩ける。

use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// ソケットファイルの場所。`app_cache_dir`はアプリ専用でありOS標準の場所なので流用する。
pub fn socket_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_cache_dir()
        .expect("no app cache dir")
        .join("debug-bridge.sock")
}

/// リスナーをバックグラウンドタスクとして起動する。起動に失敗してもこれは補助的な
/// デバッグ機能に過ぎないため、アプリ本体は止めずログのみ出す。
pub fn spawn(app: AppHandle) {
    let path = socket_path(&app);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = listen(app, &path).await {
            log::warn!("debug bridge failed to start on {}: {e}", path.display());
        }
    });
}

async fn listen(app: AppHandle, path: &Path) -> std::io::Result<()> {
    // 前回異常終了時のソケットファイルの残骸があるとbindが失敗するため、先に消しておく。
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    // 同一マシンの他ユーザーから触れないようにする(ソケットファイル自体のパーミッション)。
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    log::info!("debug bridge listening on {}", path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = handle_connection(&app, stream).await {
                log::warn!("debug bridge connection error: {e}");
            }
        });
    }
}

async fn handle_connection(app: &AppHandle, mut stream: UnixStream) -> std::io::Result<()> {
    let body = match read_http_body(&mut stream).await {
        Ok(body) => body,
        Err(e) => {
            let msg = format!("bad request: {e}");
            stream
                .write_all(http_response(400, "Bad Request", &msg).as_bytes())
                .await?;
            return Ok(());
        }
    };
    let js = String::from_utf8_lossy(&body).into_owned();
    // 実データに対して任意JS実行の口を開けているため、実行内容を必ずログに残す(監査性)。
    log::info!("debug bridge executing: {js}");

    let (status, reason, resp_body) = match eval_js(app, js).await {
        Ok(json) => (200, "OK", json),
        Err(e) => (500, "Internal Server Error", e),
    };
    stream
        .write_all(http_response(status, reason, &resp_body).as_bytes())
        .await?;
    Ok(())
}

/// メインウィンドウでJSを評価し、結果を`{"ok":true,"value":...}` /
/// `{"ok":false,"error":"..."}`形式のJSON文字列で返す。
///
/// リクエストボディはそのまま関数本体として実行される(WebDriverの`execute_script`と
/// 同様、値を受け取りたい場合は呼び出し側が明示的に`return`を書く必要がある)。式1つ
/// だけを投げても自動では値は返らない。これにより`throw`を含む複数文のスクリプトも
/// そのまま実行できる。
///
/// `eval_with_callback`はWindows側の制約で例外をコールバックに渡さない仕様のため
/// (Tauriのドキュメント参照)、評価するJS自体をtry/catchで包んでプラットフォーム
/// 非依存にエラーを拾う。
async fn eval_js(app: &AppHandle, js: String) -> Result<String, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "no main window".to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    let wrapped = format!(
        "(function(){{\
           try {{\
             return JSON.stringify({{ ok: true, value: (function(){{ {js}\n }})() }});\
           }} catch (e) {{\
             return JSON.stringify({{ ok: false, error: String(e) + (e && e.stack ? '\\n' + e.stack : '') }});\
           }}\
         }})()"
    );
    window
        .eval_with_callback(wrapped, move |result| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(result);
            }
        })
        .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "eval callback was dropped".to_string())
}

fn http_response(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.as_bytes().len()
    )
}

/// リクエストボディだけを読み取る。メソッド/パス/ヘッダーは`Content-Length`以外無視する
/// (このブリッジはデバッグ専用でありHTTP準拠を目的としないため)。
async fn read_http_body(stream: &mut UnixStream) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    let header_end = loop {
        if let Some(pos) = find_double_crlf(&buf) {
            break pos;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(std::io::Error::new(ErrorKind::InvalidData, "header too large"));
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "connection closed before headers complete",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let headers = String::from_utf8_lossy(&buf[..header_end]);
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err(std::io::Error::new(ErrorKind::InvalidData, "body too large"));
    }

    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "connection closed before body complete",
            ));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    Ok(body)
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_double_crlf() {
        assert_eq!(find_double_crlf(b"GET / HTTP/1.1\r\n\r\n"), Some(14));
        assert_eq!(find_double_crlf(b"no terminator here"), None);
    }

    #[test]
    fn http_response_has_correct_content_length() {
        let resp = http_response(200, "OK", "\u{3042}"); // マルチバイト文字でバイト長を確認
        assert!(resp.contains("Content-Length: 3\r\n"));
        assert!(resp.ends_with("\u{3042}"));
    }
}
