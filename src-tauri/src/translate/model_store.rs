//! ローカル翻訳のモデルファイル（Issue #603）。段階1は、置かれたファイルの検査だけを持つ
//! （ダウンロードと状態管理は段階2）。

use super::{invalid, ERR_MODEL_NOT_READY};
use crate::error::Result;
use std::path::{Path, PathBuf};

/// `pfnet/plamo-3-translate-0.8b-GGUF` の Q4_K_M。
pub const MODEL_FILE_NAME: &str = "plamo-3-translate-0.8b-Q4_K_M.gguf";
pub const MODEL_SIZE_BYTES: u64 = 573_126_848;

pub fn model_path(models_dir: &Path) -> PathBuf {
    models_dir.join(MODEL_FILE_NAME)
}

/// モデルが使える状態か（置かれていて、サイズが期待どおりか）。ハッシュ検証はダウンロード完了時だけ行う。
pub fn check_ready(path: &Path) -> Result<()> {
    check_ready_with(path, MODEL_SIZE_BYTES)
}

fn check_ready_with(path: &Path, expected_size: u64) -> Result<()> {
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
