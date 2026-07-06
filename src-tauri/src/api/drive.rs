//! ドライブ（添付ファイル）アップロード。`drive/files/create` は multipart/form-data で、
//! JSON ボディの `i` 同梱ではなくフォームフィールドとして token を送る（api/client とは別経路）。

use crate::api::normalize::RawFile;
use crate::domain::DriveFile;
use crate::error::{Error, Result};
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
