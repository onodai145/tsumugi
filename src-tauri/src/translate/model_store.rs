//! ローカル翻訳のモデルファイル（Issue #603）。固定値（URL・リビジョン・SHA-256・ライセンス版）、
//! 状態の型、置かれたファイルの検査、空き容量の判定を持つ。ダウンロード自体は `download.rs`。

use super::{invalid, ERR_MODEL_NOT_READY};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// `pfnet/plamo-3-translate-0.8b-GGUF` の Q4_K_M。
pub const MODEL_FILE_NAME: &str = "plamo-3-translate-0.8b-Q4_K_M.gguf";
pub const MODEL_SIZE_BYTES: u64 = 573_126_848;
/// PFN 側の更新で重みが黙って変わらないよう、リビジョン（コミット）を固定する。更新はアプリのリリースで追従する。
/// 2026-11-14 に HF API（`/api/models/pfnet/plamo-3-translate-0.8b-GGUF?blobs=true`）の `sha` と `lfs.sha256` から取得した。
/// URL・ライセンス URL に埋め込んだリビジョンと一致することを、テストが検証する（`concat!` で定数から組み立てられないため）。
#[allow(dead_code)]
pub const MODEL_REVISION: &str = "1cf96ae02133958a20074bfdfab76517bb737d00";
pub const MODEL_SHA256: &str = "70891ec70dc7ab4205745a38307867c2881b99239f20327927633abfe4bc9ec6";
pub const MODEL_URL: &str = "https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/resolve/1cf96ae02133958a20074bfdfab76517bb737d00/plamo-3-translate-0.8b-Q4_K_M.gguf";
/// 同意を求めるライセンス（PLaMo Community License、2026-11-09 改定版）の版と、全文（日本語）の写し。
pub const LICENSE_VERSION: &str = "2026-11-09";
pub const LICENSE_URL: &str =
    "https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/blob/1cf96ae02133958a20074bfdfab76517bb737d00/LICENSE/ja";
/// 空き容量に足す余裕（`.part` の書き込みのあとにリネームや他のアプリの書き込みがあっても足りるように）。
pub const FREE_SPACE_MARGIN: u64 = 256 * 1024 * 1024;

pub fn model_path(models_dir: &Path) -> PathBuf {
    models_dir.join(MODEL_FILE_NAME)
}

/// ダウンロード中のファイル。検証に通ってから `model_path` へリネームする。
pub fn part_path(models_dir: &Path) -> PathBuf {
    models_dir.join(format!("{MODEL_FILE_NAME}.part"))
}

/// モデルの状態。ダウンロード中・検証中・失敗は `LocalModels` が持ち、ディスクから分かるのは `Ready` と `NotInstalled`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LocalModelStatus {
    NotInstalled,
    Downloading {
        #[specta(type = specta_typescript::Number)]
        downloaded: u64,
        #[specta(type = specta_typescript::Number)]
        total: u64,
    },
    Verifying,
    Ready,
    Failed { reason: String },
}

/// ディスク上の状態（`Ready` か `NotInstalled`）。
pub fn status_on_disk(models_dir: &Path, expected_size: u64) -> LocalModelStatus {
    match check_ready_with(&model_path(models_dir), expected_size) {
        Ok(()) => LocalModelStatus::Ready,
        Err(_) => LocalModelStatus::NotInstalled,
    }
}

/// 残りのバイト数 + 余裕が空いているか。`.part` が期待サイズ以上なら、検証だけなので余裕は要らない。
pub fn has_enough_space(available: u64, expected_size: u64, part_len: u64) -> bool {
    let remaining = expected_size.saturating_sub(part_len);
    remaining == 0 || available >= remaining + FREE_SPACE_MARGIN
}

/// `dir` があるボリュームの空き容量。調べられなければ `None`（その場合は確認を省く）。
pub fn available_space(dir: &Path) -> Option<u64> {
    fs4::available_space(dir).ok()
}

/// モデルが使える状態か（置かれていて、サイズが期待どおりか）。ハッシュ検証はダウンロード完了時だけ行う。
pub fn check_ready(path: &Path) -> Result<()> {
    check_ready_with(path, MODEL_SIZE_BYTES)
}

pub(crate) fn check_ready_with(path: &Path, expected_size: u64) -> Result<()> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() == expected_size => Ok(()),
        _ => Err(invalid(ERR_MODEL_NOT_READY)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tsumugi_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn assert_not_ready(r: Result<()>) {
        match r {
            Err(Error::Invalid(m)) => assert_eq!(m, ERR_MODEL_NOT_READY),
            other => panic!("expected LOCAL_MODEL_NOT_READY, got {other:?}"),
        }
    }

    #[test]
    fn the_pinned_constants_match_the_official_values() {
        assert_eq!(MODEL_REVISION.len(), 40);
        assert_eq!(MODEL_SHA256.len(), 64);
        assert!(MODEL_SHA256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        // URL はリビジョンとファイル名を含み、公式リポジトリを指す（黙って別物に変わらない）。
        assert_eq!(
            MODEL_URL,
            format!("https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/resolve/{MODEL_REVISION}/{MODEL_FILE_NAME}")
        );
        assert!(LICENSE_URL.contains(MODEL_REVISION));
        assert_eq!(LICENSE_VERSION, "2026-11-09");
    }

    #[test]
    fn part_path_sits_next_to_the_model() {
        let p = part_path(Path::new("/data/models"));
        assert_eq!(p, Path::new("/data/models").join(format!("{MODEL_FILE_NAME}.part")));
    }

    #[test]
    fn status_on_disk_is_ready_only_for_the_exact_size() {
        let dir = temp_dir();
        assert_eq!(status_on_disk(&dir, 10), LocalModelStatus::NotInstalled);
        std::fs::write(model_path(&dir), b"123456789").unwrap();
        assert_eq!(status_on_disk(&dir, 10), LocalModelStatus::NotInstalled);
        std::fs::write(model_path(&dir), b"1234567890").unwrap();
        assert_eq!(status_on_disk(&dir, 10), LocalModelStatus::Ready);
    }

    #[test]
    fn free_space_needs_the_remaining_bytes_plus_a_margin() {
        let need = 1000 - 400 + FREE_SPACE_MARGIN;
        assert!(has_enough_space(need, 1000, 400));
        assert!(!has_enough_space(need - 1, 1000, 400));
        // `.part` が期待サイズ以上なら、残りは 0（検証だけ）。余裕分は要求しない。
        assert!(has_enough_space(0, 1000, 1000));
    }

    #[test]
    fn status_serializes_with_a_camel_case_tag() {
        let j = serde_json::to_value(LocalModelStatus::Downloading { downloaded: 1, total: 2 }).unwrap();
        assert_eq!(j, serde_json::json!({"type": "downloading", "downloaded": 1, "total": 2}));
        let j = serde_json::to_value(LocalModelStatus::NotInstalled).unwrap();
        assert_eq!(j, serde_json::json!({"type": "notInstalled"}));
    }

    #[test]
    fn available_space_of_an_existing_directory_is_known() {
        assert!(available_space(&temp_dir()).is_some());
    }

    #[test]
    fn model_path_is_under_the_models_dir() {
        let p = model_path(Path::new("/data/models"));
        assert_eq!(p, Path::new("/data/models").join(MODEL_FILE_NAME));
    }

    #[test]
    fn missing_file_is_not_ready() {
        let dir = temp_dir();
        assert_not_ready(check_ready_with(&dir.join("none.gguf"), 10));
    }

    #[test]
    fn wrong_size_is_not_ready() {
        let dir = temp_dir();
        let f = dir.join("m.gguf");
        std::fs::write(&f, b"123456789").unwrap();
        assert_not_ready(check_ready_with(&f, 10));
    }

    #[test]
    fn exact_size_is_ready() {
        let dir = temp_dir();
        let f = dir.join("m.gguf");
        std::fs::write(&f, b"1234567890").unwrap();
        check_ready_with(&f, 10).unwrap();
    }

    #[test]
    fn a_directory_is_not_ready() {
        let dir = temp_dir();
        assert_not_ready(check_ready_with(&dir, 0));
    }
}
