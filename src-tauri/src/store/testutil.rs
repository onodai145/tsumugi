//! 永続化のテスト用の補助。

use std::path::PathBuf;

/// 保存先のディレクトリを消して、保存を失敗させる(`repair_dir` で直る)。
pub(crate) struct BrokenDisk {
    dir: PathBuf,
}

impl BrokenDisk {
    pub(crate) fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("tsumugi-broken-disk-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub(crate) fn break_dir(&self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }

    pub(crate) fn repair_dir(&self) {
        std::fs::create_dir_all(&self.dir).unwrap();
    }
}

impl Drop for BrokenDisk {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
