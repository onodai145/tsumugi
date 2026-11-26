//! テスト用の補助(Issue #625 段階2)。アーカイブの作成と、偽のエンジン・声の配置。

use super::catalog::{EngineAssets, Vvm};
use super::store::Store;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;

pub fn sha256_hex(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

pub fn temp_root() -> PathBuf {
    let d = std::env::temp_dir().join(format!("tsumugi_vv_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    zip_bytes(entries, None)
}

pub fn make_zip_with_symlink(entries: &[(&str, &[u8])], link: (&str, &str)) -> Vec<u8> {
    zip_bytes(entries, Some(link))
}

fn zip_bytes(entries: &[(&str, &[u8])], link: Option<(&str, &str)>) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        if let Some((name, target)) = link {
            w.add_symlink(name, target, opts).unwrap();
        }
        w.finish().unwrap();
    }
    buf.into_inner()
}

/// 名前を、`..` や絶対パスも含めて、そのまま書く(`tar::Header::set_path` は拒否するので、先頭 100 バイトを直接書く)。
fn append_raw<W: Write>(
    b: &mut tar::Builder<W>,
    name: &str,
    kind: tar::EntryType,
    link: Option<&str>,
    data: &[u8],
) {
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(0o644);
    h.set_entry_type(kind);
    h.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
    if let Some(l) = link {
        h.set_link_name(l).unwrap();
    }
    h.set_cksum();
    b.append(&h, data).unwrap();
}

pub fn make_tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    make_tgz_with_symlink(entries, None)
}

pub fn make_tgz_with_symlink(entries: &[(&str, &[u8])], link: Option<(&str, &str)>) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut b = tar::Builder::new(gz);
    for (name, data) in entries {
        append_raw(&mut b, name, tar::EntryType::Regular, None, data);
    }
    if let Some((name, target)) = link {
        append_raw(&mut b, name, tar::EntryType::Symlink, Some(target), &[]);
    }
    b.into_inner().unwrap().finish().unwrap()
}

/// エンジン一式が取得済みの状態を、小さな偽のファイルで作る。
pub fn install_fake_engine(s: &Store, a: &EngineAssets) {
    for p in [s.core_lib(a), s.ort_lib(a), s.dict_dir().join("sys.dic")] {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }
    Store::write_ready(&s.engine_dir()).unwrap();
}

/// 声(VVM)が取得済みの状態を、スパース(サイズだけ合わせた中身のない)ファイルで作る。
pub fn install_fake_vvm(s: &Store, v: &Vvm) {
    std::fs::create_dir_all(s.vvm_dir()).unwrap();
    std::fs::File::create(s.vvm_path(&v.id))
        .unwrap()
        .set_len(v.size)
        .unwrap();
}
