//! 取得物(zip・tgz)の安全な展開(Issue #625 段階2)。必要なファイルだけを取り出し、
//! パスの脱出(`..`・絶対パス)・シンボリックリンク・展開後の巨大なファイルを許さない。

use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum ExtractError {
    /// `..`・絶対パスを含むエントリがあった。アーカイブ全体を拒否する。
    Unsafe,
    /// 展開後のサイズが上限を超えた。
    TooLarge,
    /// 取り出したいファイルがアーカイブになかった。
    Missing(String),
    /// アーカイブとして読めない。
    Format,
    Io(String),
}

impl ExtractError {
    /// 利用者向けの理由。パスは含めない。
    pub fn reason(&self) -> String {
        match self {
            Self::Unsafe => "ダウンロードしたファイルの内容が安全でないため、展開をやめました".into(),
            Self::TooLarge => "ダウンロードしたファイルの展開後のサイズが想定を超えています".into(),
            Self::Missing(_) => "ダウンロードしたファイルに必要なファイルが含まれていません".into(),
            Self::Format => "ダウンロードしたファイルを展開できませんでした".into(),
            Self::Io(_) => "ファイルの書き込みに失敗しました(ディスクの空き容量を確認してください)".into(),
        }
    }

    /// ログ用の短いタグ。パス・I/O の詳細は含めない。
    pub fn log_tag(&self) -> &'static str {
        match self {
            Self::Unsafe => "unsafe path",
            Self::TooLarge => "too large",
            Self::Missing(_) => "missing entry",
            Self::Format => "bad archive",
            Self::Io(_) => "io",
        }
    }
}

fn io(e: std::io::Error) -> ExtractError {
    ExtractError::Io(e.to_string())
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// 1 ファイルの展開後の最大バイト数。
    pub max_file: u64,
    /// 展開後の合計の最大バイト数。
    pub max_total: u64,
}

pub enum Select<'a> {
    /// `(アーカイブ内の、最上位のディレクトリを除いたパス, 出力のファイル名)`。ここにあるものだけを取り出す。
    Files(&'a [(&'a str, &'a str)]),
    /// 通常のファイルをすべて、ディレクトリ構造を保って取り出す。
    All,
}

/// 最上位のディレクトリ(先頭の要素)を除いた相対パス。ディレクトリ自身は `None`。
/// `..`・絶対パス・ドライブ指定を含むエントリは、取り出さないものも含めて拒否する。
fn relative(path: &Path) -> Result<Option<PathBuf>, ExtractError> {
    let mut out = PathBuf::new();
    let mut first = true;
    for c in path.components() {
        match c {
            Component::Normal(p) => {
                if first {
                    first = false;
                } else {
                    out.push(p);
                }
            }
            Component::CurDir => {}
            _ => return Err(ExtractError::Unsafe),
        }
    }
    Ok(if out.as_os_str().is_empty() { None } else { Some(out) })
}

fn key(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

/// 取り出すなら、`(Files の何番目か, 書き出し先)`。
fn destination(select: &Select<'_>, rel: &Path, dest_dir: &Path) -> Option<(Option<usize>, PathBuf)> {
    match select {
        Select::All => Some((None, dest_dir.join(rel))),
        Select::Files(wants) => {
            let k = key(rel);
            wants.iter().position(|(entry, _)| *entry == k).map(|i| (Some(i), dest_dir.join(wants[i].1)))
        }
    }
}

struct Budget {
    max_file: u64,
    left: u64,
}

fn write_entry(reader: &mut dyn Read, dest: &Path, budget: &mut Budget) -> Result<(), ExtractError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let mut f = std::fs::File::create(dest).map_err(io)?;
    let cap = budget.max_file.min(budget.left);
    // 宣言されたサイズは信じず、実際に読んだバイト数で数える(1 バイト多く読んで、超過を検出する)。
    let n = std::io::copy(&mut Read::take(&mut *reader, cap + 1), &mut f).map_err(io)?;
    if n > cap {
        return Err(ExtractError::TooLarge);
    }
    budget.left -= n;
    Ok(())
}

fn check_found(select: &Select<'_>, found: &[bool]) -> Result<(), ExtractError> {
    if let Select::Files(wants) = select {
        if let Some(i) = found.iter().position(|f| !*f) {
            return Err(ExtractError::Missing(wants[i].0.to_string()));
        }
    }
    Ok(())
}

fn found_slots(select: &Select<'_>) -> Vec<bool> {
    match select {
        Select::Files(w) => vec![false; w.len()],
        Select::All => vec![],
    }
}

/// zip から、`select` のファイルを `dest_dir` へ取り出す。失敗したら、呼び出し側が `dest_dir` ごと捨てる。
pub fn extract_zip(archive: &Path, dest_dir: &Path, select: Select<'_>, limits: Limits) -> Result<(), ExtractError> {
    let file = std::fs::File::open(archive).map_err(io)?;
    let mut z = zip::ZipArchive::new(file).map_err(|_| ExtractError::Format)?;
    let mut found = found_slots(&select);
    let mut budget = Budget { max_file: limits.max_file, left: limits.max_total };
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|_| ExtractError::Format)?;
        let name = e.name().to_string();
        let Some(rel) = relative(Path::new(&name))? else { continue };
        // 通常のファイルだけ。ディレクトリとシンボリックリンク(Unix のモードの種別が 0o120000)は取り出さない。
        let is_symlink = e.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000);
        if !e.is_file() || is_symlink {
            continue;
        }
        let Some((slot, dest)) = destination(&select, &rel, dest_dir) else { continue };
        write_entry(&mut e, &dest, &mut budget)?;
        if let Some(s) = slot {
            found[s] = true;
        }
    }
    check_found(&select, &found)
}

/// tgz から、`select` のファイルを `dest_dir` へ取り出す。
pub fn extract_tgz(archive: &Path, dest_dir: &Path, select: Select<'_>, limits: Limits) -> Result<(), ExtractError> {
    let file = std::fs::File::open(archive).map_err(io)?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut found = found_slots(&select);
    let mut budget = Budget { max_file: limits.max_file, left: limits.max_total };
    for entry in ar.entries().map_err(|_| ExtractError::Format)? {
        let mut e = entry.map_err(|_| ExtractError::Format)?;
        let path = e.path().map_err(|_| ExtractError::Format)?.into_owned();
        let Some(rel) = relative(&path)? else { continue };
        // 通常のファイルだけ(シンボリックリンク・ハードリンク・ディレクトリは取り出さない)。
        if !e.header().entry_type().is_file() {
            continue;
        }
        let Some((slot, dest)) = destination(&select, &rel, dest_dir) else { continue };
        write_entry(&mut e, &dest, &mut budget)?;
        if let Some(s) = slot {
            found[s] = true;
        }
    }
    check_found(&select, &found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::voicevox::test_util::{make_tgz, make_tgz_with_symlink, make_zip, make_zip_with_symlink, temp_root};

    const LIM: Limits = Limits { max_file: 1 << 20, max_total: 1 << 22 };

    fn put(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn count(dir: &Path) -> usize {
        std::fs::read_dir(dir).map(|r| r.count()).unwrap_or(0)
    }

    #[test]
    fn zip_extracts_only_the_wanted_file_under_the_given_name() {
        let d = temp_root();
        let a = put(&d, "a.zip", &make_zip(&[("top/lib/libx.so", b"LIB"), ("top/README.txt", b"r"), ("top/include/x.h", b"h")]));
        let out = d.join("out");
        extract_zip(&a, &out, Select::Files(&[("lib/libx.so", "libx.so")]), LIM).unwrap();
        assert_eq!(std::fs::read(out.join("libx.so")).unwrap(), b"LIB");
        assert_eq!(count(&out), 1, "ほかは取り出さない");
    }

    #[test]
    fn zip_reports_a_missing_wanted_file() {
        let d = temp_root();
        let a = put(&d, "a.zip", &make_zip(&[("top/other", b"x")]));
        let r = extract_zip(&a, &d.join("out"), Select::Files(&[("lib/libx.so", "libx.so")]), LIM);
        assert_eq!(r, Err(ExtractError::Missing("lib/libx.so".into())));
    }

    #[test]
    fn zip_rejects_a_path_that_escapes_the_destination() {
        let d = temp_root();
        let a = put(&d, "a.zip", &make_zip(&[("top/ok", b"x"), ("top/../evil.txt", b"E")]));
        let r = extract_zip(&a, &d.join("out"), Select::Files(&[("ok", "ok")]), LIM);
        assert_eq!(r, Err(ExtractError::Unsafe));
        assert!(!d.join("evil.txt").exists());
    }

    #[test]
    fn zip_rejects_an_absolute_path() {
        let d = temp_root();
        let a = put(&d, "a.zip", &make_zip(&[("/abs/evil", b"E")]));
        assert_eq!(extract_zip(&a, &d.join("out"), Select::All, LIM), Err(ExtractError::Unsafe));
    }

    #[test]
    fn zip_never_extracts_a_symlink_even_if_it_is_the_wanted_name() {
        let d = temp_root();
        let a = put(&d, "a.zip", &make_zip_with_symlink(&[("top/real", b"R")], ("top/lib/liby.so", "real")));
        let out = d.join("out");
        let r = extract_zip(&a, &out, Select::Files(&[("lib/liby.so", "liby.so")]), LIM);
        assert_eq!(r, Err(ExtractError::Missing("lib/liby.so".into())));
        assert!(!out.join("liby.so").exists());
    }

    #[test]
    fn zip_enforces_the_per_file_and_total_size_limits() {
        let d = temp_root();
        let a = put(&d, "a.zip", &make_zip(&[("top/big", &[7u8; 11])]));
        let r = extract_zip(&a, &d.join("o1"), Select::All, Limits { max_file: 10, max_total: 100 });
        assert_eq!(r, Err(ExtractError::TooLarge));
        let a = put(&d, "b.zip", &make_zip(&[("top/a", &[1u8; 6]), ("top/b", &[2u8; 6])]));
        let r = extract_zip(&a, &d.join("o2"), Select::All, Limits { max_file: 100, max_total: 10 });
        assert_eq!(r, Err(ExtractError::TooLarge));
        let r = extract_zip(&a, &d.join("o3"), Select::All, Limits { max_file: 6, max_total: 12 });
        assert_eq!(r, Ok(()), "上限ちょうどは通る");
    }

    #[test]
    fn zip_rejects_bytes_that_are_not_an_archive() {
        let d = temp_root();
        let a = put(&d, "a.zip", b"this is not a zip");
        assert_eq!(extract_zip(&a, &d.join("out"), Select::All, LIM), Err(ExtractError::Format));
    }

    #[test]
    fn tgz_extracts_the_real_file_and_ignores_the_symlink_next_to_it() {
        let d = temp_root();
        // ONNX Runtime の tgz と同じ形: lib/libx.so -> libx.so.1(シンボリックリンク)と、実体。
        let a = put(&d, "a.tgz", &make_tgz_with_symlink(&[("top/lib/libx.so.1", b"REAL"), ("top/TERMS.txt", b"t")], Some(("top/lib/libx.so", "libx.so.1"))));
        let out = d.join("out");
        extract_tgz(&a, &out, Select::Files(&[("lib/libx.so.1", "libx.so.1")]), LIM).unwrap();
        assert_eq!(std::fs::read(out.join("libx.so.1")).unwrap(), b"REAL");
        assert_eq!(count(&out), 1);
    }

    #[test]
    fn tgz_all_keeps_the_structure_and_skips_symlinks() {
        let d = temp_root();
        let a = put(&d, "a.tgz", &make_tgz_with_symlink(&[("top/a.bin", b"A"), ("top/sub/b.bin", b"B")], Some(("top/link", "a.bin"))));
        let out = d.join("out");
        extract_tgz(&a, &out, Select::All, LIM).unwrap();
        assert_eq!(std::fs::read(out.join("a.bin")).unwrap(), b"A");
        assert_eq!(std::fs::read(out.join("sub/b.bin")).unwrap(), b"B");
        assert!(!out.join("link").exists());
    }

    #[test]
    fn tgz_rejects_a_path_that_escapes_the_destination() {
        let d = temp_root();
        let a = put(&d, "a.tgz", &make_tgz(&[("top/ok", b"x"), ("top/../evil.txt", b"E")]));
        assert_eq!(extract_tgz(&a, &d.join("out"), Select::All, LIM), Err(ExtractError::Unsafe));
        assert!(!d.join("evil.txt").exists());
    }

    #[test]
    fn tgz_enforces_the_size_limits_and_rejects_non_archives() {
        let d = temp_root();
        let a = put(&d, "a.tgz", &make_tgz(&[("top/big", &[7u8; 11])]));
        assert_eq!(extract_tgz(&a, &d.join("o1"), Select::All, Limits { max_file: 10, max_total: 100 }), Err(ExtractError::TooLarge));
        let bad = put(&d, "b.tgz", b"not gzip at all");
        assert_eq!(extract_tgz(&bad, &d.join("o2"), Select::All, LIM), Err(ExtractError::Format));
    }

    #[test]
    fn the_reasons_hide_paths() {
        for e in [ExtractError::Unsafe, ExtractError::TooLarge, ExtractError::Missing("/secret/x".into()), ExtractError::Format, ExtractError::Io("/secret/y".into())] {
            assert!(!e.reason().contains("secret"), "{e:?}");
            assert!(!e.log_tag().contains("secret"), "{e:?}");
        }
    }
}
