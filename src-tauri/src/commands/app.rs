//! アプリ情報系コマンド。

use crate::error::Result;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

/// ビルド時の短縮gitコミットハッシュ（`build.rs` が `TSUMUGI_GIT_HASH` として埋め込む）。
/// `.git` が無い環境(tarball等)からのビルドでは "unknown" になる。
#[tauri::command]
#[specta::specta]
pub fn git_commit_hash() -> String {
    env!("TSUMUGI_GIT_HASH").to_string()
}

const RELEASES_REPO: &str = "onodai145/tsumugi";

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LatestRelease {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// GitHub Releases から最新版を確認する（Issue #4）。新しいバージョンがあれば返し、
/// 最新か取得に失敗した場合は None（呼び出し側を静かに無視させ、オフライン時等に
/// エラー扱いでBackstageを騒がせないようにする）。
#[tauri::command]
#[specta::specta]
pub async fn check_latest_release(state: State<'_, AppState>) -> Result<Option<LatestRelease>> {
    match fetch_latest_release(&state.http).await {
        Ok(v) => Ok(v),
        Err(e) => {
            log::warn!("update check failed: {e}");
            Ok(None)
        }
    }
}

async fn fetch_latest_release(http: &reqwest::Client) -> Result<Option<LatestRelease>> {
    let resp = http
        .get(format!("https://api.github.com/repos/{RELEASES_REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let rel: GhRelease = resp.json().await?;
    if rel.draft || rel.prerelease {
        return Ok(None);
    }
    let latest_version = rel.tag_name.trim_start_matches('v');
    if !is_newer(latest_version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    Ok(Some(LatestRelease {
        version: latest_version.to_string(),
        url: rel.html_url,
    }))
}

/// "x.y.z" 形式のバージョン文字列を数値ごとに比較する（semverクレートを追加するほどでもない
/// 単純な用途のため自前実装）。パース不能な区切りは 0 として扱う。
fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    }
    parts(latest) > parts(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_compares_numerically() {
        assert!(is_newer("0.3.10", "0.3.9"));
        assert!(!is_newer("0.3.9", "0.3.10"));
        assert!(!is_newer("0.3.3", "0.3.3"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }
}
