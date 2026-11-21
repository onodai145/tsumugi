//! 内蔵 VOICEVOX CORE の固定値(Issue #625 段階2)。取得物の URL・サイズ・SHA-256、VVM の中身(キャラ・スタイル)、
//! 規約の写しと同意版。更新はアプリのリリースで追従する(VVM の部分は `scripts/gen-voicevox-catalog.py` で再生成する)。

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub const CORE_VERSION: &str = "0.17.1";
/// CORE 0.17.1 の推奨は 1.23.2(prerelease)だが、パスを明示すれば 1.17.3(正式リリースの最新)で動く(スパイクノート)。
pub const ORT_VERSION: &str = "1.17.3";
pub const DICT_VERSION: &str = "1.11";
/// エンジン一式(CORE + ONNX Runtime + 辞書)の識別子。置き場のディレクトリ名に使う。版を変えるときは一緒に変える。
pub const ENGINE_SET: &str = "core0.17.1-ort1.17.3-dict1.11";

const CATALOG_JSON: &str = include_str!("catalog.json");
const TERMS_VOICE_MODELS: &str = include_str!("terms/voice-models.txt");
const TERMS_ONNXRUNTIME: &str = include_str!("terms/onnxruntime.txt");

#[derive(Debug, Clone, Deserialize)]
pub struct Style {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Character {
    pub name: String,
    /// クレジット表記(例 `VOICEVOX:ずんだもん`)。規約から取り出した値。
    pub credit: String,
    pub styles: Vec<Style>,
}

/// 声のモデル(VVM)1 個。`id` は `0`〜`25`、`n0`。
#[derive(Debug, Clone, Deserialize)]
pub struct Vvm {
    pub id: String,
    pub size: u64,
    pub sha256: String,
    pub characters: Vec<Character>,
}

#[derive(Debug, Deserialize)]
pub struct Catalog {
    pub vvm_release: String,
    pub vvms: Vec<Vvm>,
}

impl Catalog {
    pub fn vvm(&self, id: &str) -> Option<&Vvm> {
        self.vvms.iter().find(|v| v.id == id)
    }

    /// スタイル ID から、それを持つ VVM・キャラ・スタイルを引く。
    pub fn voice(&self, style_id: u32) -> Option<(&Vvm, &Character, &Style)> {
        for v in &self.vvms {
            for c in &v.characters {
                for s in &c.styles {
                    if s.id == style_id {
                        return Some((v, c, s));
                    }
                }
            }
        }
        None
    }
}

pub fn catalog() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| serde_json::from_str(CATALOG_JSON).expect("catalog.json is valid"))
}

pub fn vvm_url(id: &str) -> String {
    format!("https://github.com/VOICEVOX/voicevox_vvm/releases/download/{}/{id}.vvm", catalog().vvm_release)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// 環境ごとのエンジン一式の取得物。`core_lib` / `ort_lib` は、アーカイブの `lib/` 直下のファイル名。
#[derive(Debug, Clone, PartialEq)]
pub struct EngineAssets {
    pub core: Asset,
    pub ort: Asset,
    pub dict: Asset,
    pub core_lib: &'static str,
    pub ort_lib: &'static str,
}

/// 対応する環境(`std::env::consts::{OS, ARCH}` の値)ごとの取得物。対応しない組は `None`。
pub fn engine_assets_for(os: &str, arch: &str) -> Option<EngineAssets> {
    // (CORE の配布名の環境, ONNX Runtime の配布名の環境, CORE のライブラリ, ONNX Runtime のライブラリ,
    //  CORE の (サイズ, SHA-256), ONNX Runtime の (サイズ, SHA-256))
    let (core_plat, ort_plat, core_lib, ort_lib, (core_size, core_sha), (ort_size, ort_sha)) = match (os, arch) {
        ("linux", "x86_64") => (
            "linux-x64", "linux-x64", "libvoicevox_core.so", "libvoicevox_onnxruntime.so.1.17.3",
            (1_457_717, "844b49ce48040ecd85c8b2baaf75f77855277a0f92f6e87ea7ec45c820943323"),
            (6_765_720, "72b5287fdd48dc833a9929f6e9e3826e793b54ce1202181be93f63823a222f58"),
        ),
        ("linux", "aarch64") => (
            "linux-arm64", "linux-arm64", "libvoicevox_core.so", "libvoicevox_onnxruntime.so.1.17.3",
            (1_413_960, "3a5e9876df26fdece0ddb8de9be09f438b4c884dfd0b58d248b27c91e0e6c807"),
            (6_177_662, "276eedc007b694324f59bc35c2cf9041724eb786608437a649f096edea3943a9"),
        ),
        ("macos", "x86_64") => (
            "osx-x64", "osx-x86_64", "libvoicevox_core.dylib", "libvoicevox_onnxruntime.1.17.3.dylib",
            (1_261_129, "8bbf3c7ff790a787b8cf6db1f654ec39155b1401831ff46051448a8a670e89ce"),
            (5_886_886, "59edc860252dbb7f64cc2eb72bd4b2904a6df7c35f33dc8082435b9f15a505df"),
        ),
        ("macos", "aarch64") => (
            "osx-arm64", "osx-arm64", "libvoicevox_core.dylib", "libvoicevox_onnxruntime.1.17.3.dylib",
            (1_188_914, "b3959b18d64f0039e402b4f70f14291a182910bd739eeb6130d1a0324742644d"),
            (5_061_505, "96d7ea1928a0a15485492d37621387df50f40a792e2a6817d4df73ecd18571f2"),
        ),
        ("windows", "x86_64") => (
            "windows-x64", "win-x64", "voicevox_core.dll", "voicevox_onnxruntime.dll",
            (1_462_186, "7e6a4f6d39c0a083e694a9191289df3b09725abfa676027228a72c01a2ae565b"),
            (4_650_448, "32b07e7c6c59030434b54eb0255912510942faf193c7a14e0e1a168db5796265"),
        ),
        _ => return None,
    };
    Some(EngineAssets {
        core: Asset {
            url: format!("https://github.com/VOICEVOX/voicevox_core/releases/download/{CORE_VERSION}/voicevox_core-{core_plat}-{CORE_VERSION}.zip"),
            size: core_size,
            sha256: core_sha.into(),
        },
        ort: Asset {
            url: format!("https://github.com/VOICEVOX/onnxruntime-builder/releases/download/voicevox_onnxruntime-{ORT_VERSION}/voicevox_onnxruntime-{ort_plat}-{ORT_VERSION}.tgz"),
            size: ort_size,
            sha256: ort_sha.into(),
        },
        dict: Asset {
            url: "https://github.com/r9y9/open_jtalk/releases/download/v1.11.1/open_jtalk_dic_utf_8-1.11.tar.gz".into(),
            size: 23_646_843,
            sha256: "fe6ba0e43542cef98339abdffd903e062008ea170b04e7e2a35da805902f382a".into(),
        },
        core_lib,
        ort_lib,
    })
}

/// このビルド・実行環境の取得物。対応しなければ `None`(Android など)。
pub fn engine_assets() -> Option<EngineAssets> {
    engine_assets_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// 埋め込んだ 2 つの規約の写し。
pub struct Terms {
    pub voice_models: &'static str,
    pub onnxruntime: &'static str,
}

pub fn terms() -> Terms {
    Terms { voice_models: TERMS_VOICE_MODELS, onnxruntime: TERMS_ONNXRUNTIME }
}

fn version_of(voice_models: &str, onnxruntime: &str) -> String {
    // 改行コード(Windows のチェックアウトの CRLF)で版が変わらないよう、除いてからハッシュする。
    let mut h = Sha256::new();
    h.update(b"voice-models\n");
    h.update(voice_models.replace('\r', "").as_bytes());
    h.update(b"\nonnxruntime\n");
    h.update(onnxruntime.replace('\r', "").as_bytes());
    format!("{:x}", h.finalize())
}

/// 同意版。埋め込んだ規約の写しが変わると変わる(利用者に再同意を求める)。
pub fn terms_version() -> String {
    version_of(TERMS_VOICE_MODELS, TERMS_ONNXRUNTIME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_has_the_27_talk_voice_models_and_no_singing_only_one() {
        let c = catalog();
        assert_eq!(c.vvm_release, "0.17.1");
        assert_eq!(c.vvms.len(), 27);
        assert!(c.vvm("s0").is_none(), "歌声だけの VVM は含めない");
        assert!(c.vvm("0").is_some() && c.vvm("25").is_some() && c.vvm("n0").is_some());
        let total: u64 = c.vvms.iter().map(|v| v.size).sum();
        assert!((1_600_000_000..1_700_000_000).contains(&total), "合計 {total}");
    }

    #[test]
    fn style_ids_are_unique_and_every_vvm_has_a_valid_hash_and_size() {
        let c = catalog();
        let mut ids = std::collections::HashSet::new();
        for v in &c.vvms {
            assert_eq!(v.sha256.len(), 64, "{}", v.id);
            assert!(v.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()), "{}", v.id);
            assert!(v.size > 1_000_000, "{}", v.id);
            assert!(!v.characters.is_empty(), "{}", v.id);
            for ch in &v.characters {
                assert!(!ch.credit.is_empty() && !ch.styles.is_empty(), "{}", ch.name);
                for s in &ch.styles {
                    assert!(ids.insert(s.id), "スタイル ID {} が重複している", s.id);
                }
            }
        }
        assert_eq!(ids.len(), 137);
    }

    #[test]
    fn a_style_id_resolves_to_its_owner_voice_model_character_and_style() {
        let (v, c, s) = catalog().voice(3).unwrap();
        assert_eq!((v.id.as_str(), c.name.as_str(), s.name.as_str()), ("0", "ずんだもん", "ノーマル"));
        assert!(catalog().voice(99_999).is_none());
        assert!(catalog().voice(3000).is_none(), "歌声のスタイルは含めない");
    }

    #[test]
    fn credits_come_from_the_terms() {
        assert_eq!(catalog().voice(3).unwrap().1.credit, "VOICEVOX:ずんだもん");
        assert_eq!(catalog().voice(10000).unwrap().1.credit, "VOICEVOX Nemo");
        assert_eq!(catalog().voice(10).unwrap().1.credit, "VOICEVOX:雨晴はう", "規約に節がないキャラは既定の表記");
    }

    #[test]
    fn every_supported_platform_resolves_assets_and_others_do_not() {
        for (os, arch) in [("linux", "x86_64"), ("linux", "aarch64"), ("macos", "x86_64"), ("macos", "aarch64"), ("windows", "x86_64")] {
            let a = engine_assets_for(os, arch).unwrap_or_else(|| panic!("{os} {arch}"));
            for asset in [&a.core, &a.ort, &a.dict] {
                assert!(asset.url.starts_with("https://github.com/"), "{}", asset.url);
                assert_eq!(asset.sha256.len(), 64);
                assert!(asset.size > 1_000_000);
            }
            assert!(a.core.url.contains("0.17.1") && a.ort.url.contains("1.17.3"));
            assert!(!a.core_lib.is_empty() && !a.ort_lib.is_empty());
        }
        for (os, arch) in [("linux", "riscv64"), ("android", "aarch64"), ("windows", "aarch64"), ("freebsd", "x86_64"), ("ios", "aarch64")] {
            assert!(engine_assets_for(os, arch).is_none(), "{os} {arch}");
        }
    }

    #[test]
    fn the_library_names_match_the_archives() {
        let l = engine_assets_for("linux", "x86_64").unwrap();
        assert_eq!((l.core_lib, l.ort_lib), ("libvoicevox_core.so", "libvoicevox_onnxruntime.so.1.17.3"));
        let m = engine_assets_for("macos", "aarch64").unwrap();
        assert_eq!((m.core_lib, m.ort_lib), ("libvoicevox_core.dylib", "libvoicevox_onnxruntime.1.17.3.dylib"));
        let w = engine_assets_for("windows", "x86_64").unwrap();
        assert_eq!((w.core_lib, w.ort_lib), ("voicevox_core.dll", "voicevox_onnxruntime.dll"));
    }

    #[test]
    fn the_terms_version_is_stable_and_ignores_line_endings() {
        assert_eq!(version_of("a\r\nb", "c"), version_of("a\nb", "c"));
        assert_ne!(version_of("a", "c"), version_of("a", "d"));
        assert_ne!(version_of("a", "bc"), version_of("ab", "c"), "2 つの規約の境目を区別する");
        let v = terms_version();
        assert_eq!(v.len(), 64);
        assert_eq!(v, terms_version());
        assert!(terms().voice_models.contains("VOICEVOX 音声モデル 利用規約"));
        assert!(terms().onnxruntime.contains("VOICEVOX ONNX Runtime 利用規約"));
    }

    /// 埋め込んだ規約の写しが、上流と食い違っていないか。ネットワークが要る。
    #[tokio::test]
    #[ignore]
    async fn the_embedded_terms_match_upstream() {
        let norm = |s: &str| s.replace('\r', "");
        let vm = reqwest::get("https://github.com/VOICEVOX/voicevox_vvm/releases/download/0.17.1/TERMS.txt")
            .await.unwrap().text().await.unwrap();
        assert_eq!(norm(&vm), norm(terms().voice_models));
    }
}
