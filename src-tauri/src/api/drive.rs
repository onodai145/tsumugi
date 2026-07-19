//! ドライブ（添付ファイル）アップロード。`drive/files/create` は multipart/form-data で、
//! JSON ボディの `i` 同梱ではなくフォームフィールドとして token を送る（api/client とは別経路）。

use crate::api::normalize::RawFile;
use crate::api::MisskeyClient;
use crate::domain::{DriveFile, SourceItem};
use crate::error::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

/// ローカルファイルを Misskey ドライブへアップロードし、DriveFile を返す。
pub async fn upload_file(
    http: &reqwest::Client,
    host: &str,
    token: &str,
    path: &str,
) -> Result<DriveFile> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| Error::Invalid(format!("cannot read file {path}: {e}")))?;
    let filename = Path::new(path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());

    let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
    let form = reqwest::multipart::Form::new()
        .text("i", token.to_string())
        .part("file", part);

    let url = format!("https://{host}/api/drive/files/create");
    let resp = http.post(&url).multipart(form).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let code = resp
            .text()
            .await
            .ok()
            .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
            .and_then(|v| v.get("error").and_then(|e| e.get("code")).and_then(|c| c.as_str()).map(str::to_string))
            .unwrap_or_default();
        return Err(match status.as_u16() {
            401 => Error::Unauthorized(format!("drive/files/create: {code}")),
            403 => Error::Forbidden(format!("drive/files/create: {code}")),
            413 => Error::Api("drive/files/create: file too large".into()),
            429 => Error::RateLimited,
            _ => Error::Api(format!("drive/files/create: {} {code}", status.as_u16())),
        });
    }

    let raw: RawFile = resp.json().await?;
    Ok(raw.into())
}

/// 1ページあたりの取得件数。フロント側の「もっと見る」判定（返却件数がこの値未満なら
/// 終端とみなす）と一致させる。
const DRIVE_LIST_LIMIT: u8 = 30;

fn list_files_body(folder_id: Option<&str>, until_id: Option<&str>) -> Value {
    let mut body = json!({ "limit": DRIVE_LIST_LIMIT, "folderId": folder_id });
    if let Some(u) = until_id {
        body["untilId"] = json!(u);
    }
    body
}

/// ドライブのファイル一覧。`folder_id: None` はルート直下、`until_id` はページング
/// （直前に取得した最後のファイルIDを渡す）。種別フィルタはかけない
/// （Misskey の投稿添付に種別制限は無いため）。
pub async fn list_files(
    client: &MisskeyClient,
    folder_id: Option<&str>,
    until_id: Option<&str>,
) -> Result<Vec<DriveFile>> {
    let raw: Vec<RawFile> = client
        .post("drive/files", &list_files_body(folder_id, until_id))
        .await?;
    Ok(raw.into_iter().map(DriveFile::from).collect())
}

fn list_folders_body(folder_id: Option<&str>) -> Value {
    json!({ "limit": 100, "folderId": folder_id })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFolder {
    id: String,
    #[serde(default)]
    name: String,
}

/// 指定フォルダ直下のサブフォルダ一覧（`folder_id: None` はルート直下）。
pub async fn list_folders(client: &MisskeyClient, folder_id: Option<&str>) -> Result<Vec<SourceItem>> {
    let raw: Vec<RawFolder> = client
        .post("drive/folders", &list_folders_body(folder_id))
        .await?;
    Ok(raw
        .into_iter()
        .map(|f| SourceItem { id: f.id, name: f.name })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_files_body_root_has_null_folder_id_and_no_until_id() {
        let body = list_files_body(None, None);
        assert_eq!(body["limit"], 30);
        assert_eq!(body["folderId"], Value::Null);
        assert!(body.get("untilId").is_none());
    }

    #[test]
    fn list_files_body_includes_folder_and_until_id_when_present() {
        let body = list_files_body(Some("f1"), Some("n9"));
        assert_eq!(body["folderId"], "f1");
        assert_eq!(body["untilId"], "n9");
    }

    #[test]
    fn list_folders_body_root_has_null_folder_id() {
        let body = list_folders_body(None);
        assert_eq!(body["limit"], 100);
        assert_eq!(body["folderId"], Value::Null);
    }

    #[test]
    fn list_folders_body_includes_folder_id_when_present() {
        let body = list_folders_body(Some("f1"));
        assert_eq!(body["folderId"], "f1");
    }
}
