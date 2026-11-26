//! 内蔵 VOICEVOX CORE の取得物の置き場(Issue #625 段階2)。`<root>` は `app_data_dir/voicevox`。
//!
//! ```text
//! <root>/engine-<ENGINE_SET>/{core/<lib>, ort/<lib>, dict/*, READY}   エンジン一式(READY は最後に置く)
//! <root>/engine-<ENGINE_SET>.tmp/                                      取得中(完了で engine-<ENGINE_SET> へリネーム)
//! <root>/vvm-<VVM のリリース>/<id>.vvm                                 声
//! <root>/dl/<名前>.<SHA-256 の先頭 8 文字>.part                        取得中のファイル(Range で再開できる)
//! ```

use super::catalog::{catalog, engine_assets, EngineAssets, Vvm, ENGINE_SET};
use std::path::{Path, PathBuf};

const READY_FILE: &str = "READY";

pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    #[cfg(test)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn engine_dir(&self) -> PathBuf {
        self.root.join(format!("engine-{ENGINE_SET}"))
    }

    pub fn engine_tmp_dir(&self) -> PathBuf {
        self.root.join(format!("engine-{ENGINE_SET}.tmp"))
    }

    pub fn core_lib(&self, a: &EngineAssets) -> PathBuf {
        self.engine_dir().join("core").join(a.core_lib)
    }

    pub fn ort_lib(&self, a: &EngineAssets) -> PathBuf {
        self.engine_dir().join("ort").join(a.ort_lib)
    }

    pub fn dict_dir(&self) -> PathBuf {
        self.engine_dir().join("dict")
    }

    /// エンジン一式の完成の印を置く。ディレクトリの中身がそろったあと、最後に呼ぶ。
    pub fn write_ready(dir: &Path) -> std::io::Result<()> {
        std::fs::write(dir.join(READY_FILE), ENGINE_SET)
    }

    /// エンジン一式が使える状態か。READY(最後に置く)と、必要なファイルがそろっていること。
    pub fn engine_ready(&self, a: &EngineAssets) -> bool {
        let is_file = |p: PathBuf| std::fs::metadata(p).is_ok_and(|m| m.is_file());
        is_file(self.engine_dir().join(READY_FILE))
            && is_file(self.core_lib(a))
            && is_file(self.ort_lib(a))
            && is_file(self.dict_dir().join("sys.dic"))
    }

    pub fn vvm_dir(&self) -> PathBuf {
        self.root.join(format!("vvm-{}", catalog().vvm_release))
    }

    pub fn vvm_path(&self, id: &str) -> PathBuf {
        self.vvm_dir().join(format!("{id}.vvm"))
    }

    /// 声のモデルが使える状態か(置かれていて、サイズが期待どおり)。ハッシュ検証は取得の完了時だけ行う。
    pub fn vvm_ready(&self, v: &Vvm) -> bool {
        std::fs::metadata(self.vvm_path(&v.id)).is_ok_and(|m| m.is_file() && m.len() == v.size)
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join("dl")
    }

    /// 取得中のファイル。名前にハッシュの先頭 8 文字を含める(版が変わったあとに、古い `.part` へ継ぎ足さないため)。
    pub fn part_path(&self, file_name: &str, sha256: &str) -> PathBuf {
        let tag: String = sha256.chars().take(8).collect();
        self.downloads_dir().join(format!("{file_name}.{tag}.part"))
    }

    /// `file_name` の `.part`(全ハッシュ分)。削除に使う。
    pub fn part_files(&self, file_name: &str) -> Vec<PathBuf> {
        let prefix = format!("{file_name}.");
        let Ok(rd) = std::fs::read_dir(self.downloads_dir()) else {
            return vec![];
        };
        rd.filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                n.starts_with(&prefix) && n.ends_with(".part")
            })
            .map(|e| e.path())
            .collect()
    }

    pub fn partial_bytes(&self, file_name: &str, sha256: &str) -> u64 {
        match std::fs::metadata(self.part_path(file_name, sha256)) {
            Ok(m) if m.is_file() => m.len(),
            _ => 0,
        }
    }

    /// 古い版のディレクトリと、いまの取得物に対応しない `.part` を消す。起動時に、ロードより前に呼ぶ。
    /// 失敗は無視する(Windows ではロード中の DLL のあるディレクトリを消せない)。
    pub fn clean_stale(&self) {
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return;
        };
        let keep_engine = format!("engine-{ENGINE_SET}");
        let keep_vvm = format!("vvm-{}", catalog().vvm_release);
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let stale = (name.starts_with("engine-") && name != keep_engine)
                || (name.starts_with("vvm-") && name != keep_vvm);
            if stale && e.path().is_dir() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
        // いまのハッシュの先頭 8 文字(エンジン一式の 3 つ + 全 VVM)。
        let mut tags: Vec<String> = catalog()
            .vvms
            .iter()
            .map(|v| v.sha256.chars().take(8).collect())
            .collect();
        if let Some(a) = engine_assets() {
            tags.extend(
                [&a.core, &a.ort, &a.dict]
                    .iter()
                    .map(|x| x.sha256.chars().take(8).collect::<String>()),
            );
        }
        let Ok(rd) = std::fs::read_dir(self.downloads_dir()) else {
            return;
        };
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".part") else {
                continue;
            };
            let current = stem
                .rsplit('.')
                .next()
                .is_some_and(|tag| tags.iter().any(|t| t == tag));
            if !current {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::voicevox::catalog::{catalog, engine_assets_for};

    fn temp_root() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tsumugi_vv_store_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn assets() -> EngineAssets {
        engine_assets_for("linux", "x86_64").unwrap()
    }

    fn touch(p: &Path, len: u64) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let f = std::fs::File::create(p).unwrap();
        f.set_len(len).unwrap();
    }

    fn install_engine(s: &Store, a: &EngineAssets) {
        touch(&s.core_lib(a), 10);
        touch(&s.ort_lib(a), 10);
        touch(&s.dict_dir().join("sys.dic"), 10);
        std::fs::write(s.engine_dir().join(READY_FILE), "x").unwrap();
    }

    #[test]
    fn the_engine_is_ready_only_when_the_marker_and_every_file_exist() {
        let s = Store::new(temp_root());
        let a = assets();
        assert!(!s.engine_ready(&a));
        install_engine(&s, &a);
        assert!(s.engine_ready(&a));
        std::fs::remove_file(s.engine_dir().join(READY_FILE)).unwrap();
        assert!(
            !s.engine_ready(&a),
            "READY がなければ、途中のディレクトリは使わない"
        );
        std::fs::write(s.engine_dir().join(READY_FILE), "x").unwrap();
        std::fs::remove_file(s.dict_dir().join("sys.dic")).unwrap();
        assert!(!s.engine_ready(&a), "辞書が欠けていれば未完了");
    }

    #[test]
    fn a_voice_model_is_ready_only_when_its_size_matches() {
        let s = Store::new(temp_root());
        let v = Vvm {
            id: "0".into(),
            size: 1000,
            sha256: "a".repeat(64),
            characters: vec![],
        };
        assert!(!s.vvm_ready(&v));
        touch(&s.vvm_path("0"), 999);
        assert!(!s.vvm_ready(&v), "サイズが違えば未取得");
        touch(&s.vvm_path("0"), 1000);
        assert!(s.vvm_ready(&v));
    }

    #[test]
    fn part_files_are_named_by_file_and_hash_prefix_and_counted() {
        let s = Store::new(temp_root());
        let sha = "0123456789abcdef".repeat(4);
        let p = s.part_path("0.vvm", &sha);
        assert_eq!(
            p.file_name().unwrap().to_str().unwrap(),
            "0.vvm.01234567.part"
        );
        assert_eq!(s.partial_bytes("0.vvm", &sha), 0);
        touch(&p, 42);
        assert_eq!(s.partial_bytes("0.vvm", &sha), 42);
        touch(&s.part_path("0.vvm", &"f".repeat(64)), 5);
        touch(&s.part_path("1.vvm", &sha), 5);
        assert_eq!(
            s.part_files("0.vvm").len(),
            2,
            "別のハッシュの .part も含む。別のファイルのは含まない"
        );
    }

    #[test]
    fn clean_stale_removes_old_versions_and_unrelated_parts_but_keeps_the_current_ones() {
        let s = Store::new(temp_root());
        let a = assets();
        install_engine(&s, &a);
        touch(&s.vvm_path("0"), 5);
        let keep_sha = catalog().vvms[0].sha256.clone();
        touch(&s.part_path("0.vvm", &keep_sha), 5);
        // 古い版・途中のもの・知らない .part。
        touch(&s.root().join("engine-core0.0.1-old").join("x"), 1);
        touch(
            &s.root().join(format!("engine-{ENGINE_SET}.tmp")).join("x"),
            1,
        );
        touch(&s.root().join("vvm-0.0.1").join("0.vvm"), 1);
        touch(&s.downloads_dir().join("0.vvm.deadbeef.part"), 1);
        touch(&s.root().join("unrelated-file"), 1);

        s.clean_stale();

        assert!(s.engine_ready(&a));
        assert!(s.vvm_path("0").exists());
        assert!(s.part_path("0.vvm", &keep_sha).exists());
        assert!(!s.root().join("engine-core0.0.1-old").exists());
        assert!(!s.root().join(format!("engine-{ENGINE_SET}.tmp")).exists());
        assert!(!s.root().join("vvm-0.0.1").exists());
        assert!(!s.downloads_dir().join("0.vvm.deadbeef.part").exists());
        assert!(
            s.root().join("unrelated-file").exists(),
            "無関係なファイルは触らない"
        );
    }

    #[test]
    fn clean_stale_on_a_missing_root_does_nothing() {
        Store::new(
            std::env::temp_dir().join(format!("tsumugi_vv_missing_{}", uuid::Uuid::new_v4())),
        )
        .clean_stale();
    }
}
