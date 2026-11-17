# Android 触覚入り ogg の通知音 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Android で、触覚チャンネル入りの ogg(Vorbis)を通知音に選ぶと、通知時と「試聴」で、音と一緒に端末が振動するようにする(Issue #592)。

**Architecture:** Rust の純粋関数 `has_haptic_channel` が ogg の Vorbis コメントから `ANDROID_HAPTIC` を判定する。`play_notify_sound` は、Android かつ触覚入りの音だけを、既存の `tauri-plugin-haptics` に足す `play_haptic_audio`(Kotlin の `MediaPlayer`)へ渡す。それ以外は今まで通り rodio。触覚の ON/OFF は既存の `UiPrefs.haptics_enabled` に従い、OFF なら触覚ミュートのまま音だけ鳴らす。

**Tech Stack:** Rust(Tauri v2、rodio)、tauri-plugin-haptics(自前プラグイン、Kotlin)、Android `MediaPlayer` / `MediaDataSource`、ffmpeg(fixture 生成)

**Spec:** `docs/superpowers/specs/2026-11-17-android-haptic-ogg-design.md`(承認済み。判断に迷ったらこれが正)

## Global Constraints

- ブランチは `feat/issue-592-android-haptic-ogg`(作成済み)。`main` には直接コミットしない。
- コミットメッセージは**件名のみ**(Conventional Commits、日本語、本文なし)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` のトレーラーを付ける。`git commit` が失敗・タイムアウトしたら止めて報告する(再試行しない)。`--no-verify` は使わない。
- 対象は Android だけ。デスクトップの動作は変えない(触覚入り ogg も今まで通り rodio に渡す)。
- 触覚入りと判定するのは **Vorbis の ogg だけ**(先頭 `OggS`、先頭 64KiB 内の `\x03vorbis` コメントヘッダ、`ANDROID_HAPTIC=<1 以上の整数>`、キーは大文字小文字を区別しない)。Opus は対象外。
- `MediaPlayer` の `AudioAttributes` は usage=`USAGE_NOTIFICATION`、contentType=`CONTENT_TYPE_SONIFICATION`、`setHapticChannelsMuted(!hapticsEnabled)`。データは `MediaDataSource`(メモリ上のバイト列)で渡し、ファイルは作らない。
- Kotlin の `invoke` は、どの経路でも必ず `resolve` / `reject` のどちらかを一度だけ呼ぶ(Rust 側が固まらないため)。準備のタイムアウトは 5 秒。
- 触覚入りの音は後勝ち(新しい要求は、再生中の `MediaPlayer` を止める)。Kotlin 側の失敗は Rust 側で `log::warn!` + beep(`PRESET_BEEP`)で代替する。`play_notify_sound` は常に `Ok(())` を返す。
- `bindings/tauri.gen.ts` は手で編集しない。`play_notify_sound` に `app: AppHandle` を足しても、TS バインディングは変わらないはず(差分が出たら原因を調べる)。
- `llama.cpp` のビルドは重い。`cargo build` / `cargo test` は `CARGO_BUILD_JOBS=4` を付け、`CARGO_TARGET_DIR` を `/tmp` に置かない。Android のビルドは NDK r27c(`~/Android/Sdk/ndk/27.2.12479018`)で、`ANDROID_NDK_ROOT` と `ANDROID_PLATFORM=android-26` を必ず export する(CLAUDE.md 参照)。
- テストコマンドは `cd src-tauri && cargo test --lib <名前>`。`cargo run` / `./target/debug/tsumugi` は実行しない。
- 実機(Pixel 8 Pro)には、`adb install` で自分のアプリを上書きする前に、署名の不一致でアンインストールが必要になるかを確認する。アンインストールするとアプリのデータが消えるので、その場合は実行前にユーザーに確認する。

## Review Focus

1. **触覚タグ付きだが壊れた / 切れた ogg**(コメントヘッダが 64KiB の外、コメント数や長さが巨大、途中で切れている)→ パニックせず false(または正しい true)を返す。Task 1 のテストで固定する。
2. **触覚なしの音**(プリセットの WAV、MML が生成する WAV、Opus、先頭が `OggS` でないもの)→ 必ず従来経路。Task 1 のテストで固定する。
3. **通知が連続して、再生中に次が来る**→ 前の呼び出しが固まらず、beep も重ならない(置き換えられた再生は失敗ではなく成功として終える)。Task 2 の実装で担保し、Task 4 の受け入れ基準 4 で確認する。
4. **Kotlin 側の失敗・タイムアウト**→ Rust が固まらず、beep で代替して通知は止まらない。Task 2 の実装で担保し、Task 4 の受け入れ基準 5 で確認する。
5. **設定のハプティクスが OFF**→ 触覚ミュートのまま音だけ鳴る(`MediaPlayer` を使うが振動しない)。Task 4 の受け入れ基準 2 で確認する。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `src-tauri/src/sound.rs`(変更) | `has_haptic_channel`(純粋関数)とそのテストを追加する |
| `src-tauri/tests/fixtures/haptic_ogg/haptic_3ch.ogg`, `plain_2ch.ogg`(新規) | 実エンコーダが作った触覚入り / 触覚なしの ogg(各約 4.5KB) |
| `src-tauri/plugins/tauri-plugin-haptics/src/models.rs`(変更) | `PlayHapticAudioRequest`(モバイルのみ) |
| `src-tauri/plugins/tauri-plugin-haptics/src/mobile.rs`(変更) | `Haptics::play_haptic_audio(&[u8], bool)` |
| `src-tauri/plugins/tauri-plugin-haptics/Cargo.toml`(変更) | `base64` を追加 |
| `src-tauri/plugins/tauri-plugin-haptics/android/.../HapticAudioPlayer.kt`(新規) | `MediaPlayer` による再生(後勝ち、タイムアウト、必ず一度だけ結果を返す) |
| `src-tauri/plugins/tauri-plugin-haptics/android/.../HapticsPlugin.kt`(変更) | `playHapticAudio` コマンドの受け口 |
| `src-tauri/src/commands/sound.rs`(変更) | Android で触覚入りの音を振り分け、失敗時は beep |
| `docs/guide/user-guide.md`(変更) | 触覚入り ogg の説明 |

---

### Task 1: 触覚入り ogg の判定 `has_haptic_channel`

**Files:**
- Create: `src-tauri/tests/fixtures/haptic_ogg/haptic_3ch.ogg`, `src-tauri/tests/fixtures/haptic_ogg/plain_2ch.ogg`
- Modify: `src-tauri/src/sound.rs`(関数は `decode_fully` の後、テストは既存の `mod tests` の末尾)

**Interfaces:**
- Consumes: なし
- Produces: `pub(crate) fn has_haptic_channel(bytes: &[u8]) -> bool`(Task 3 が `crate::sound::has_haptic_channel` として呼ぶ)

- [ ] **Step 1: fixture を作る**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
mkdir -p src-tauri/tests/fixtures/haptic_ogg && cd src-tauri/tests/fixtures/haptic_ogg
ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i "sine=frequency=440:duration=0.1:sample_rate=44100" \
  -f lavfi -i "sine=frequency=440:duration=0.1:sample_rate=44100" \
  -f lavfi -i "aevalsrc='0.9*sin(2*PI*175*t)':d=0.1:s=44100" \
  -filter_complex "[0][1][2]amerge=inputs=3,channelmap=channel_layout=3.0" \
  -c:a libvorbis -metadata ANDROID_HAPTIC=1 haptic_3ch.ogg
ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i "sine=frequency=440:duration=0.1:sample_rate=44100" \
  -ac 2 -c:a libvorbis plain_2ch.ogg
ls -l *.ogg
ffprobe -v error -show_entries stream=channels:stream_tags -of default=nw=1 haptic_3ch.ogg
```

Expected: 2 ファイルが各 4.5KB 前後で、ffprobe が `channels=3` と `TAG:ANDROID_HAPTIC=1` を出す。

- [ ] **Step 2: 失敗するテストを書く**

`src-tauri/src/sound.rs` の既存 `#[cfg(test)] mod tests { use super::*; ... }` の末尾(最後の `}` の直前)に追加する。

```rust
    // ---- has_haptic_channel (Issue #592) ----

    const HAPTIC_OGG: &[u8] = include_bytes!("../tests/fixtures/haptic_ogg/haptic_3ch.ogg");
    const PLAIN_OGG: &[u8] = include_bytes!("../tests/fixtures/haptic_ogg/plain_2ch.ogg");

    /// 判定の単体テスト用に、`padding` バイトの後ろに Vorbis コメントヘッダを持つ ogg 風のバイト列を作る
    /// (`has_haptic_channel` はヘッダしか見ないので、音声データは要らない)。
    fn ogg_with_comments_after(padding: usize, comments: &[&str]) -> Vec<u8> {
        let mut v = b"OggS".to_vec();
        v.extend(std::iter::repeat(0u8).take(padding));
        v.extend_from_slice(b"\x03vorbis");
        let vendor = b"test";
        v.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        v.extend_from_slice(vendor);
        v.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            v.extend_from_slice(&(c.len() as u32).to_le_bytes());
            v.extend_from_slice(c.as_bytes());
        }
        v.push(1); // framing bit
        v
    }

    fn ogg_with_comments(comments: &[&str]) -> Vec<u8> {
        ogg_with_comments_after(23, comments)
    }

    #[test]
    fn haptic_fixture_is_detected_and_plain_one_is_not() {
        assert!(has_haptic_channel(HAPTIC_OGG));
        assert!(!has_haptic_channel(PLAIN_OGG));
    }

    #[test]
    fn haptic_tag_values() {
        assert!(has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=1"])));
        assert!(has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=2"])));
        // 他のコメントと並んでいても見つかる
        assert!(has_haptic_channel(&ogg_with_comments(&["TITLE=x", "ANDROID_HAPTIC=1", "ARTIST=y"])));
        // 0・数値でない・負・u32 を超える・空は触覚なし
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=0"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=abc"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=-1"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=99999999999"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC="])));
        // '=' が無い・キーが似ているだけのものは別のコメント
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["XANDROID_HAPTIC=1"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC_X=1"])));
    }

    #[test]
    fn haptic_tag_key_is_case_insensitive() {
        assert!(has_haptic_channel(&ogg_with_comments(&["android_haptic=1"])));
        assert!(has_haptic_channel(&ogg_with_comments(&["Android_Haptic=1"])));
    }

    #[test]
    fn non_ogg_and_non_vorbis_are_never_haptic() {
        assert!(!has_haptic_channel(&[]));
        assert!(!has_haptic_channel(b"Ogg"));
        assert!(!has_haptic_channel(b"RIFF\x24\x00\x00\x00WAVEfmt "));
        // 先頭が OggS でなければ、タグがあっても false
        let mut not_ogg = ogg_with_comments(&["ANDROID_HAPTIC=1"]);
        not_ogg[0] = b'X';
        assert!(!has_haptic_channel(&not_ogg));
        // Opus(OpusTags)は対象外。\x03vorbis ヘッダが無いので false
        let mut opus = ogg_with_comments(&["ANDROID_HAPTIC=1"]);
        let p = opus.windows(7).position(|w| w == b"\x03vorbis").unwrap();
        opus[p..p + 7].copy_from_slice(b"OpusTag");
        assert!(!has_haptic_channel(&opus));
    }

    #[test]
    fn preset_sounds_are_never_haptic() {
        // 通知音のプリセット(WAV)と、MML が生成する WAV は従来経路(rodio)のまま
        for bytes in [
            include_bytes!("../assets/sounds/beep.wav").as_slice(),
            include_bytes!("../assets/sounds/chime.wav").as_slice(),
            include_bytes!("../assets/sounds/ping.wav").as_slice(),
            include_bytes!("../assets/sounds/pop.wav").as_slice(),
        ] {
            assert!(!has_haptic_channel(bytes));
        }
    }

    #[test]
    fn comment_header_beyond_the_scan_window_is_not_detected() {
        // 先頭 64KiB の外にあるコメントヘッダは見ない(既知の制約)
        assert!(has_haptic_channel(&ogg_with_comments_after(60_000, &["ANDROID_HAPTIC=1"])));
        assert!(!has_haptic_channel(&ogg_with_comments_after(70_000, &["ANDROID_HAPTIC=1"])));
    }

    #[test]
    fn corrupt_comment_lengths_do_not_panic() {
        // ベンダー文字列の長さが巨大
        let mut v = b"OggS".to_vec();
        v.extend_from_slice(&[0u8; 23]);
        v.extend_from_slice(b"\x03vorbis");
        v.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(!has_haptic_channel(&v));
        // コメント数が巨大で、中身が足りない
        let mut v = b"OggS".to_vec();
        v.extend_from_slice(&[0u8; 23]);
        v.extend_from_slice(b"\x03vorbis");
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(!has_haptic_channel(&v));
        // コメントの長さが残りより長い
        let mut v = b"OggS".to_vec();
        v.extend_from_slice(&[0u8; 23]);
        v.extend_from_slice(b"\x03vorbis");
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&1000u32.to_le_bytes());
        v.extend_from_slice(b"ANDROID_HAPTIC=1");
        assert!(!has_haptic_channel(&v));
    }

    #[test]
    fn every_truncation_of_a_real_file_is_safe() {
        // どの長さで切れていてもパニックしない。コメントヘッダが読める長さ以降は true
        for n in 0..=HAPTIC_OGG.len() {
            let _ = has_haptic_channel(&HAPTIC_OGG[..n]);
        }
        assert!(!has_haptic_channel(&HAPTIC_OGG[..50]));
        // 先頭 700 バイト(コメントヘッダは読めて、音声データは壊れている)。受け入れ基準 5 の壊れた ogg に使う
        assert!(has_haptic_channel(&HAPTIC_OGG[..700]));
    }
```

- [ ] **Step 3: テストが失敗(コンパイルエラー)することを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib has_haptic 2>&1 | tail -15`
Expected: `cannot find function 'has_haptic_channel' in this scope` でコンパイルに失敗する。

- [ ] **Step 4: 最小の実装を書く**

`src-tauri/src/sound.rs` の `decode_fully` 関数の直後(`open_sink` のドキュメントコメントの前)に追加する。

```rust
/// コメントヘッダを探す範囲。触覚入り ogg のコメントヘッダは先頭近くにあるので、
/// 通知のたびに音声全体を舐めないよう先頭だけを見る。
const HAPTIC_SCAN_LIMIT: usize = 64 * 1024;

/// ogg(Vorbis)が Android の触覚チャンネル入りかを判定する(Issue #592)。
///
/// 触覚入りの ogg は、Vorbis コメントに `ANDROID_HAPTIC=<触覚チャンネル数>` を持つ
/// (AOSP「UX foundation for haptic framework」)。次をすべて満たすときだけ true:
/// 先頭が `OggS` / 先頭 64KiB に Vorbis のコメントヘッダ(`\x03vorbis`)がある /
/// `ANDROID_HAPTIC`(大文字小文字を区別しない)の値が 1 以上の整数。
///
/// 既知の制約: コメントヘッダが 64KiB の外にある、または Ogg のページ境界をまたぐ(カバー画像入りなど)
/// ときは判定できず false(触覚なしの音として rodio で鳴る)。Opus は対象外。
/// 壊れたデータでもパニックせず false を返す。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn has_haptic_channel(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"OggS") {
        return false;
    }
    let window = &bytes[..bytes.len().min(HAPTIC_SCAN_LIMIT)];
    let Some(pos) = window.windows(7).position(|w| w == b"\x03vorbis") else {
        return false;
    };
    let comments = &window[pos + 7..];
    let mut cursor = 0usize;
    let Some(vendor_len) = read_u32_le(comments, &mut cursor) else {
        return false;
    };
    if read_slice(comments, &mut cursor, vendor_len as usize).is_none() {
        return false;
    }
    let Some(count) = read_u32_le(comments, &mut cursor) else {
        return false;
    };
    // count が巨大でも、各反復で必ず読み進めるか None で抜けるので、バッファの長さで止まる
    for _ in 0..count {
        let Some(len) = read_u32_le(comments, &mut cursor) else {
            return false;
        };
        let Some(comment) = read_slice(comments, &mut cursor, len as usize) else {
            return false;
        };
        if let Some(is_haptic) = parse_haptic_comment(comment) {
            return is_haptic;
        }
    }
    false
}

fn read_u32_le(buf: &[u8], pos: &mut usize) -> Option<u32> {
    let end = pos.checked_add(4)?;
    let value = u32::from_le_bytes(buf.get(*pos..end)?.try_into().ok()?);
    *pos = end;
    Some(value)
}

fn read_slice<'a>(buf: &'a [u8], pos: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = pos.checked_add(len)?;
    let slice = buf.get(*pos..end)?;
    *pos = end;
    Some(slice)
}

/// `KEY=VALUE` のコメントが `ANDROID_HAPTIC` なら、値が 1 以上の整数かを返す。別のキーなら None。
fn parse_haptic_comment(comment: &[u8]) -> Option<bool> {
    let eq = comment.iter().position(|&b| b == b'=')?;
    let (key, value) = (&comment[..eq], &comment[eq + 1..]);
    if !key.eq_ignore_ascii_case(b"ANDROID_HAPTIC") {
        return None;
    }
    let channels = std::str::from_utf8(value).ok().and_then(|s| s.parse::<u32>().ok());
    Some(channels.is_some_and(|n| n >= 1))
}
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib sound::tests 2>&1 | tail -25`
Expected: `sound.rs` の既存 3 件と新規 8 件が PASS(フィルタが `commands::sound::tests` にも一致するので、合計は 20 件前後になる)。FAILED が 0 件であればよい。

もし `every_truncation_of_a_real_file_is_safe` の `[..700]` が false で落ちたら、fixture のコメントヘッダが 700 バイトより後ろにある。`python3 -I -c "b=open('src-tauri/tests/fixtures/haptic_ogg/haptic_3ch.ogg','rb').read(); print(b.find(b'ANDROID_HAPTIC'))"` で位置を確かめ、700 を「その位置 + 20 以上、ファイル長未満」の値に直す(確認時点では 164)。

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/sound.rs src-tauri/tests/fixtures/haptic_ogg
git commit -m "feat(sound): ogg の触覚チャンネル(ANDROID_HAPTIC)を判定する(Issue #592)"
```

(Co-Authored-By トレーラーを付ける)

---

### Task 2: プラグインの `play_haptic_audio`(Rust と Kotlin)

**Files:**
- Modify: `src-tauri/plugins/tauri-plugin-haptics/Cargo.toml`, `src/models.rs`, `src/mobile.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/android/src/main/java/com/onodai/tsumugi/haptics/HapticAudioPlayer.kt`
- Modify: `src-tauri/plugins/tauri-plugin-haptics/android/src/main/java/com/onodai/tsumugi/haptics/HapticsPlugin.kt`

**Interfaces:**
- Consumes: なし(Task 1 とは独立)
- Produces: `Haptics<R>::play_haptic_audio(&self, data: &[u8], haptics_enabled: bool) -> tauri_plugin_haptics::Result<()>`(モバイルのみ。`HapticsExt` の `app.haptics()` 経由。Task 3 が使う)。Kotlin の `@Command playHapticAudio` は、再生が始まれば `resolve`、失敗・タイムアウト・API 28 以下は `reject`。

Kotlin には自動テストの基盤がないので、このタスクの検証は「Android 向けの `cargo check` と Kotlin のコンパイルが通ること」と、Task 4 の実機確認になる。

- [ ] **Step 1: `base64` を追加する**

`src-tauri/plugins/tauri-plugin-haptics/Cargo.toml` の `[dependencies]` の `thiserror = "2"` の下に追加する。

```toml
# 触覚入りの通知音のバイト列を Kotlin へ JSON で渡すための base64(本体の src-tauri と同じメジャー)。
base64 = "0.23"
```

- [ ] **Step 2: リクエスト型を足す**

`src-tauri/plugins/tauri-plugin-haptics/src/models.rs` の末尾に追加する。

```rust
/// 触覚入りの通知音の再生要求(Issue #592)。`data` は音声のバイト列の base64。
/// モバイルでしか使わないので、デスクトップでは dead_code にならないよう cfg で絞る。
#[cfg(mobile)]
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayHapticAudioRequest {
    pub data: String,
    pub haptics_enabled: bool,
}
```

- [ ] **Step 3: Rust のメソッドを足す**

`src-tauri/plugins/tauri-plugin-haptics/src/mobile.rs` を次のように変更する。

先頭の `use` を差し替える:

```rust
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::{HapticPattern, PlayHapticAudioRequest, VibrateRequest};
```

`impl<R: Runtime> Haptics<R>` に、`vibrate` の後ろへメソッドを追加する:

```rust
    /// 触覚入りの音声(ogg)を、触覚つきで再生する(Issue #592)。
    /// `haptics_enabled` が false のときは触覚をミュートして音だけ鳴らす。
    /// Kotlin 側が再生を始めたら `Ok`、失敗・タイムアウトなら `Err`。呼び出しは同期でブロックする。
    pub fn play_haptic_audio(&self, data: &[u8], haptics_enabled: bool) -> crate::Result<()> {
        self.0
            .run_mobile_plugin(
                "playHapticAudio",
                PlayHapticAudioRequest {
                    data: STANDARD.encode(data),
                    haptics_enabled,
                },
            )
            .map_err(Into::into)
    }
```

- [ ] **Step 4: Kotlin の再生クラスを書く**

`src-tauri/plugins/tauri-plugin-haptics/android/src/main/java/com/onodai/tsumugi/haptics/HapticAudioPlayer.kt` を新規作成する。

```kotlin
package com.onodai.tsumugi.haptics

import android.media.AudioAttributes
import android.media.MediaDataSource
import android.media.MediaPlayer
import android.os.Build
import android.os.Handler
import android.os.Looper

/**
 * 触覚チャンネル入りの音声(ogg)を MediaPlayer で再生する(Issue #592)。
 *
 * - 最新の 1 つだけを保持する(後勝ち)。新しい要求は、再生中のものを止めてから始める。
 * - MediaPlayer の生成・操作・コールバックは、すべてメインスレッドで行う。
 * - onResult は、要求ごとに必ず 1 回だけ呼ぶ: 再生が始まった(または後続の要求に置き換えられた)ら null、
 *   失敗(準備エラー・タイムアウト・API 28 以下)なら例外。呼び出し側(Rust)をブロックしたままにしないため。
 */
internal class HapticAudioPlayer {
    private val main = Handler(Looper.getMainLooper())
    private var current: Session? = null // メインスレッドからだけ触る

    fun play(data: ByteArray, hapticsEnabled: Boolean, onResult: (Throwable?) -> Unit) {
        main.post { start(data, hapticsEnabled, onResult) }
    }

    private fun start(data: ByteArray, hapticsEnabled: Boolean, onResult: (Throwable?) -> Unit) {
        current?.release() // 後勝ち。置き換えられた要求は成功として終える
        current = null
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            // setHapticChannelsMuted は API 29(Android 10)以降
            onResult(UnsupportedOperationException("haptic channels need API 29 or later"))
            return
        }
        val session = Session(data, hapticsEnabled, onResult)
        current = session
        session.begin()
    }

    private inner class Session(
        private val data: ByteArray,
        private val hapticsEnabled: Boolean,
        private val onResult: (Throwable?) -> Unit,
    ) {
        private val player = MediaPlayer()
        private var settled = false
        private var released = false
        private val timeout = Runnable {
            settle(IllegalStateException("MediaPlayer did not become ready within ${PREPARE_TIMEOUT_MS}ms"))
            release()
        }

        fun begin() {
            try {
                player.setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_NOTIFICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .setHapticChannelsMuted(!hapticsEnabled)
                        .build()
                )
                player.setDataSource(BytesDataSource(data))
                player.setOnPreparedListener {
                    if (!released) {
                        it.start()
                        settle(null)
                    }
                }
                player.setOnCompletionListener { release() }
                player.setOnErrorListener { _, what, extra ->
                    settle(IllegalStateException("MediaPlayer error what=$what extra=$extra"))
                    release()
                    true
                }
                main.postDelayed(timeout, PREPARE_TIMEOUT_MS)
                player.prepareAsync()
            } catch (e: Exception) {
                settle(e)
                release()
            }
        }

        /** 結果を 1 回だけ返す。2 回目以降は何もしない。 */
        private fun settle(error: Throwable?) {
            if (settled) return
            settled = true
            main.removeCallbacks(timeout)
            onResult(error)
        }

        fun release() {
            if (released) return
            released = true
            settle(null) // まだ結果を返していなければ(置き換えられたとき)成功として終える
            main.removeCallbacks(timeout)
            try {
                player.stop()
            } catch (_: IllegalStateException) {
                // 準備前などで stop できない状態。release だけすればよい
            }
            player.release()
            if (current === this) current = null
        }
    }

    /** メモリ上のバイト列を MediaPlayer に渡す。Context が要らず、ファイルも作らない。 */
    private class BytesDataSource(private val data: ByteArray) : MediaDataSource() {
        override fun readAt(position: Long, buffer: ByteArray, offset: Int, size: Int): Int {
            if (position >= data.size) return -1
            val n = minOf(size.toLong(), data.size - position).toInt()
            System.arraycopy(data, position.toInt(), buffer, offset, n)
            return n
        }

        override fun getSize(): Long = data.size.toLong()

        override fun close() {}
    }

    private companion object {
        const val PREPARE_TIMEOUT_MS = 5_000L
    }
}
```

- [ ] **Step 5: プラグインのコマンドを足す**

`src-tauri/plugins/tauri-plugin-haptics/android/src/main/java/com/onodai/tsumugi/haptics/HapticsPlugin.kt` を変更する。

import に追加する(既存の `import android.os.VibratorManager` の下):

```kotlin
import android.util.Base64
```

`VibrateArgs` クラスの下に追加する:

```kotlin
@InvokeArg
class PlayHapticAudioArgs {
    var data: String? = null
    var hapticsEnabled: Boolean = true
}
```

`HapticsPlugin` クラスの本体の先頭(`private val vibrator ...` の前)に追加する:

```kotlin
    private val hapticAudio = HapticAudioPlayer()
```

クラスの末尾(`vibrate` コマンドの後ろ)に追加する:

```kotlin
    /// 触覚入りの音声を MediaPlayer で再生する(Issue #592)。再生が始まったら resolve、失敗したら reject。
    @Command
    fun playHapticAudio(invoke: Invoke) {
        val args = invoke.parseArgs(PlayHapticAudioArgs::class.java)
        val bytes = try {
            Base64.decode(args.data ?: "", Base64.DEFAULT)
        } catch (e: IllegalArgumentException) {
            invoke.reject("invalid base64 audio data")
            return
        }
        if (bytes.isEmpty()) {
            invoke.reject("empty audio data")
            return
        }
        hapticAudio.play(bytes, args.hapticsEnabled) { error ->
            if (error == null) invoke.resolve() else invoke.reject(error.message ?: "haptic audio playback failed")
        }
    }
```

- [ ] **Step 6: デスクトップのビルドが壊れていないことを確認する**

Run: `cd src-tauri/plugins/tauri-plugin-haptics && cargo check 2>&1 | tail -5`
Expected: `Finished`(警告なし。`PlayHapticAudioRequest` は `cfg(mobile)` なのでデスクトップでは存在しない)。

- [ ] **Step 7: Android 向けの Rust のビルドが通ることを確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri
export ANDROID_NDK_ROOT=$HOME/Android/Sdk/ndk/27.2.12479018 ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018 NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018 ANDROID_PLATFORM=android-26
B=$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin
export PATH=$B:$PATH CC_aarch64_linux_android=$B/aarch64-linux-android26-clang CXX_aarch64_linux_android=$B/aarch64-linux-android26-clang++ AR_aarch64_linux_android=$B/llvm-ar
CARGO_BUILD_JOBS=4 cargo check --lib --target aarch64-linux-android --no-default-features --features custom-protocol 2>&1 | tail -15
```

Expected: `Finished`。`mobile.rs` の型エラーはここで出る(`cfg(mobile)` は Android ターゲットで有効)。

- [ ] **Step 8: Kotlin がコンパイルできることを確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/gen/android
JAVA_HOME=/usr/lib/jvm/java-21-openjdk ./gradlew tasks --all 2>&1 | grep -i "haptics.*compileDebugKotlin"
JAVA_HOME=/usr/lib/jvm/java-21-openjdk ./gradlew :tauri-plugin-haptics:compileDebugKotlin 2>&1 | tail -15
```

Expected: `BUILD SUCCESSFUL`。タスクのパスが違ったら 1 つ目の出力のパスに直す。Gradle の設定が古くてタスクが見つからないときは、`cd src-tauri && JAVA_HOME=/usr/lib/jvm/java-21-openjdk cargo tauri android build --debug --target aarch64`(CLAUDE.md の Android ビルド。重いので `systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G` を付ける)でビルドして、Kotlin のエラーを確認する。

- [ ] **Step 9: コミットする**

```bash
git add src-tauri/plugins/tauri-plugin-haptics src-tauri/Cargo.lock
git commit -m "feat(haptics): 触覚入りの音声を MediaPlayer で再生する play_haptic_audio を追加(Issue #592)"
```

(Co-Authored-By トレーラーを付ける。`Cargo.lock` に差分が無ければ `git add` から外す)

---

### Task 3: `play_notify_sound` の振り分け

**Files:**
- Modify: `src-tauri/src/commands/sound.rs`

**Interfaces:**
- Consumes: `crate::sound::has_haptic_channel(&[u8]) -> bool`(Task 1)、`HapticsExt::haptics().play_haptic_audio(&[u8], bool) -> Result<()>`(Task 2。Android のみ)、`AppState.settings.load_ui() -> Result<UiPrefs>`(`UiPrefs.haptics_enabled: bool`)、`AppState.sound.play(Vec<u8>)`
- Produces: なし(`play_notify_sound` のフロントからの呼び出しは変わらない)

- [ ] **Step 1: コマンドを変更する**

`src-tauri/src/commands/sound.rs` の先頭の `use tauri::State;` を差し替える:

```rust
use tauri::{AppHandle, State};
#[cfg(target_os = "android")]
use tauri_plugin_haptics::HapticsExt;
```

`play_notify_sound` を次のように差し替える(ドキュメントコメントも更新する)。

```rust
/// 通知音を鳴らす。choice は プリセットID / data URL(カスタム音声)。
/// 失敗しても通知フロー全体を止めないため、常に Ok を返す(失敗はログのみ)。
///
/// Android で、触覚チャンネル入りの ogg(`has_haptic_channel`)だけは、rodio ではなく
/// tauri-plugin-haptics の MediaPlayer で、音と一緒に振動させて鳴らす(Issue #592)。
/// それ以外(プリセット・MML・触覚なしの音、デスクトップのすべて)は今まで通り rodio。
#[tauri::command]
#[specta::specta]
pub async fn play_notify_sound(
    #[cfg_attr(not(target_os = "android"), allow(unused_variables))] app: AppHandle,
    state: State<'_, AppState>,
    choice: String,
) -> Result<()> {
    if choice.starts_with(mml::SOUND_CHOICE_PREFIX) {
        // フロント(ipc.ts)が mml: を data: に解決してから呼ぶ設計。ここに届くのは解決失敗の取りこぼし。
        log::warn!("通知音: MML がレンダリングされずに届いたため beep で代替します");
    }
    match resolve_audio_bytes(&choice) {
        Ok(bytes) => {
            let bytes = bytes.into_owned();
            #[cfg(target_os = "android")]
            {
                if crate::sound::has_haptic_channel(&bytes) {
                    play_haptic_audio(&app, &state, bytes).await;
                    return Ok(());
                }
            }
            state.sound.play(bytes);
        }
        // 注: この warn は `enable_file_logging` 設定(既定オフ)が有効な場合のみ
        // ファイルに残る。lib.rs がそれを条件にログプラグイン登録を行っているため。
        Err(e) => log::warn!("通知音: 再生対象の解決に失敗: {e}"),
    }
    Ok(())
}

/// 触覚入りの音を、tauri-plugin-haptics 経由で MediaPlayer で鳴らす(Android のみ)。
/// 触覚の ON/OFF は設定のハプティクス(`UiPrefs.haptics_enabled`)に従う(読めなければ既定の true)。
/// 失敗(Kotlin 側のエラー・タイムアウト・API 28 以下)したら、通知を無音にしないよう beep で代替する。
#[cfg(target_os = "android")]
async fn play_haptic_audio(app: &AppHandle, state: &AppState, bytes: Vec<u8>) {
    let haptics_enabled = state
        .settings
        .load_ui()
        .map(|ui| ui.haptics_enabled)
        .unwrap_or(true);
    let handle = app.clone();
    // run_mobile_plugin は結果が返るまで同期でブロックするので、async のワーカーを塞がないよう別スレッドで呼ぶ
    let result = tauri::async_runtime::spawn_blocking(move || {
        handle.haptics().play_haptic_audio(&bytes, haptics_enabled)
    })
    .await;
    let failure = match result {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e.to_string(),
        Err(e) => e.to_string(),
    };
    log::warn!("通知音: 触覚入りの音の再生に失敗したため beep で代替します: {failure}");
    state.sound.play(PRESET_BEEP.to_vec());
}
```

- [ ] **Step 2: デスクトップのビルドとテストが通ることを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib commands::sound 2>&1 | tail -15`
Expected: 既存のテスト(`resolves_known_presets` など 9 件)が PASS し、警告(unused など)が出ない。

- [ ] **Step 3: TS バインディングが変わっていないことを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib generates_frontend_bindings 2>&1 | tail -5; cd .. && git status --short frontend/src/bindings`
Expected: テストが PASS し、`git status` が何も出さない(`tauri.gen.ts` に差分なし)。差分が出たら、`app: AppHandle` 以外が原因なので調べる。

- [ ] **Step 4: Android 向けの Rust のビルドが通ることを確認する**

Task 2 の Step 7 と同じ環境変数を export して実行する。

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo check --lib --target aarch64-linux-android --no-default-features --features custom-protocol 2>&1 | tail -15`
Expected: `Finished`、警告なし(`has_haptic_channel` が Android では使われ、`play_haptic_audio` も使われる)。

- [ ] **Step 5: clippy を通す**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo clippy --lib --no-default-features --features custom-protocol 2>&1 | tail -15`
Expected: 新たな警告なし。(Android ターゲットの clippy は、Step 4 の `check` で型は確認済みなので省略してよい。)

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/commands/sound.rs
git commit -m "feat(sound): Androidで触覚入りoggの通知音を触覚つきで再生する(Issue #592)"
```

(Co-Authored-By トレーラーを付ける)

---

### Task 4: ユーザーガイドと実機での受け入れ確認

**Files:**
- Modify: `docs/guide/user-guide.md`、`docs/superpowers/specs/2026-11-17-android-haptic-ogg-design.md`(受け入れ基準 6 の結果)

**Interfaces:**
- Consumes: Task 1〜3 の実装
- Produces: なし

- [ ] **Step 1: Android のデバッグビルドを作る**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri
export ANDROID_NDK_ROOT=$HOME/Android/Sdk/ndk/27.2.12479018 ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018 NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018 ANDROID_PLATFORM=android-26
B=$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin
export PATH=$B:$PATH CC_aarch64_linux_android=$B/aarch64-linux-android26-clang CXX_aarch64_linux_android=$B/aarch64-linux-android26-clang++ AR_aarch64_linux_android=$B/llvm-ar
JAVA_HOME=/usr/lib/jvm/java-21-openjdk CARGO_BUILD_JOBS=4 systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G \
  cargo tauri android build --debug --target aarch64 2>&1 | tail -20
find gen/android/app/build/outputs -name '*.apk' | head
```

Expected: ビルドが成功し、`.apk` のパスが出る(初回は llama.cpp のビルドで時間がかかる)。`dlopen failed: cannot locate symbol "_ZNSt6__ndk113__hash_memoryEPKvm"` を避けるため、`ANDROID_NDK_ROOT` の export を忘れないこと。

- [ ] **Step 2: テスト用の ogg を作って端末へ送る**

```bash
mkdir -p /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/7421d4f5-c625-4b87-ad4a-30c65404ebe5/scratchpad/accept && cd $_
ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i "sine=frequency=440:duration=1.5:sample_rate=44100" \
  -f lavfi -i "sine=frequency=440:duration=1.5:sample_rate=44100" \
  -f lavfi -i "aevalsrc='0.9*sin(2*PI*175*t)*(0.5+0.5*sin(2*PI*4*t))':d=1.5:s=44100" \
  -filter_complex "[0][1][2]amerge=inputs=3,channelmap=channel_layout=3.0" \
  -c:a libvorbis -metadata ANDROID_HAPTIC=1 haptic.ogg
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "sine=frequency=660:duration=1.5:sample_rate=44100" -ac 2 -c:a libvorbis plain.ogg
head -c 700 /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/tests/fixtures/haptic_ogg/haptic_3ch.ogg > broken_haptic.ogg
adb devices -l
adb push haptic.ogg plain.ogg broken_haptic.ogg /sdcard/Download/
```

Expected: 端末が 1 台見え、3 ファイルが `/sdcard/Download/` に入る。

- [ ] **Step 3: アプリをインストールする**

```bash
APK=$(find /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/gen/android/app/build/outputs -name '*.apk' | head -1)
adb install -r "$APK" 2>&1 | tail -3
```

Expected: `Success`。`INSTALL_FAILED_UPDATE_INCOMPATIBLE`(署名の不一致)が出たら、アンインストールするとアプリのデータが消えるので、**ここで止めてユーザーに確認する**。

- [ ] **Step 4: 受け入れ基準 1〜6 を確認する**

端末を**通常モード・通知音量あり**にして、アプリの設定→通知で、通知音に `/sdcard/Download/haptic.ogg`(「音声ファイルを選ぶ」)を設定する。振動の有無は、システムの振動履歴で確かめる。

```bash
S=$(adb devices | awk 'NR>1 && $2=="device"{print $1; exit}')
vib() { T=$1; adb -s $S shell 'dumpsys vibrator_manager' | grep -E "^\s+11-[0-9]+ ([0-9:.]+) \|" | awk -v t="$T" '{ if ($2 >= t) print }' | grep -E "external" | sed -E 's/\| scale:.*\| usage:/| usage:/' | cut -c1-220; }
mark() { adb -s $S shell date +%H:%M:%S; }
```

1. 「試聴」を押す。`T=$(mark)` を押す直前に取り、押した後に `vib $T` を実行する。Expected: tsumugi の uid で `external` が出る(usage=NOTIFICATION)。端末も振動する。続けて、通知(自分宛のリアクションなど)を実際に受けて同じ確認をする。
2. アプリ設定→モバイルで「ハプティクス(振動)を有効にする」を OFF にして、同じ「試聴」を行う。Expected: 音は鳴り、`vib` は何も出さない。確認したら ON に戻す。
3. 通知音を `plain.ogg`、プリセット、MML に切り替えて「試聴」する。Expected: 今まで通り鳴り、`vib` は何も出さない。
4. 通知音を `haptic.ogg` にして、「試聴」を 1 秒間隔で 3 回続けて押す。Expected: アプリが落ちず、最後の音だけが最後まで鳴る(前の音は止まる)。`adb -s $S logcat -d -t 100 | grep -iE "tsumugi|haptic|FATAL"` に `FATAL` がない。
5. 通知音を `broken_haptic.ogg` にして「試聴」する。Expected: beep が鳴り、通知は止まらない。`adb -s $S logcat -d -t 200 | grep -i "触覚入りの音の再生に失敗"` が出る(`enable_file_logging` が OFF ならログは出ないので、その場合は beep が鳴ったことだけを確認する)。
6. 端末をマナーモード(バイブ)にして、通知音を `haptic.ogg` にして「試聴」する。`vib $T` で `external` が出るか、音が出ないかを確認し、結果を控える。確認後、端末のモードを元に戻す。

どれかが期待と違ったら、先へ進まずに原因を調べる(superpowers:systematic-debugging)。

- [ ] **Step 5: ユーザーガイドを更新する**

`docs/guide/user-guide.md` の、`保存した MML は、設定→通知のグローバル設定と、…` の段落(「## 検索」の直前)と「## 検索」の間に、次の節を追加する。マナーモードの文は、Step 4 の 6 の結果で A か B のどちらか一方を選んで書く。

```markdown
#### Android: 触覚入りの ogg で、音と一緒に振動させる

Android では、触覚チャンネルを持つ ogg(Vorbis)を通知音に選ぶと、音と一緒に端末が振動します。選び方は、ほかの音声ファイルと同じです(設定→通知、またはタブごとの通知音で「音声ファイルを選ぶ」)。「試聴」でも振動します。

- 触覚入りかどうかは、ogg の `ANDROID_HAPTIC`(触覚チャンネル数)のタグで判断します。タグが無い ogg や、ogg 以外(mp3・wav など)は、今までどおり音だけ鳴ります。
- 振動するかどうかは、設定→モバイルの「ハプティクス(振動)を有効にする」に従います。OFF のときは、触覚入りの ogg でも音だけ鳴ります。
- 端末が音声連動の触覚に対応している必要があります。対応していない端末の挙動は確認していません。端末によって異なります。
- 触覚入りの通知音が続けて鳴るときは、前の音を止めて、最新の音だけを鳴らします。
- 音は、システムの「通知」の音量に従います。<<マナーモードの文>>
- 触覚入りの ogg は、たとえば次のように ffmpeg で作れます(ステレオの音 `sound.wav` と、モノラルの触覚用の波形 `haptic.wav` を、この順に 3 チャンネルへまとめます。触覚チャンネルは音声チャンネルの後ろに置きます)。

  ```
  ffmpeg -i sound.wav -i haptic.wav -filter_complex "[0:a][1:a]amerge=inputs=2,channelmap=channel_layout=3.0" -c:a libvorbis -metadata ANDROID_HAPTIC=1 out.ogg
  ```

  作ったファイルが触覚入りになっているかは、`ffprobe -show_entries stream=channels:stream_tags out.ogg` で、`channels=3` と `TAG:ANDROID_HAPTIC=1` が出ることで確かめられます。
- デスクトップ版では、触覚入りの ogg を選んでも振動しません(今までどおりの再生です)。
```

`<<マナーモードの文>>` は、Step 4 の 6 の結果で、次のどちらかの文に置き換える(`<<` `>>` の記号ごと消す)。

- A(マナーモードでも `external` 振動が出た): `マナーモード(バイブ)でも、音は出ませんが、触覚入りの通知音は振動します。`
- B(マナーモードで振動しなかった): `マナーモード(バイブ)のときは、音も振動も出ません。`

次に、設定画面の説明を更新する。`docs/guide/user-guide.md` の、次の 1 行を探す。

```
- **モバイル**: スマホ版でのみ表示されるタブ。ハプティクス（振動）のON/OFF。ONの場合、ノート投稿時とリアクション付与時に端末を振動させます（既定ON）。
```

を、次に置き換える。

```
- **モバイル**: スマホ版でのみ表示されるタブ。ハプティクス（振動）のON/OFF。ONの場合、ノート投稿時とリアクション付与時に端末を振動させます（既定ON）。触覚入りの ogg を通知音にしたときの振動も、この設定に従います（上の「Android: 触覚入りの ogg で、音と一緒に振動させる」を参照）。
```

追記した ffmpeg のコマンドは、実際に動かして確かめる。

Run:
```bash
cd /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/7421d4f5-c625-4b87-ad4a-30c65404ebe5/scratchpad/accept
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "sine=frequency=440:duration=0.5:sample_rate=44100" -ac 2 sound.wav
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "sine=frequency=175:duration=0.5:sample_rate=44100" -ac 1 haptic.wav
ffmpeg -hide_banner -loglevel error -y -i sound.wav -i haptic.wav -filter_complex "[0:a][1:a]amerge=inputs=2,channelmap=channel_layout=3.0" -c:a libvorbis -metadata ANDROID_HAPTIC=1 out.ogg
ffprobe -v error -show_entries stream=channels:stream_tags -of default=nw=1 out.ogg
```
Expected: `channels=3` と `TAG:ANDROID_HAPTIC=1`。出なければ、ドキュメントのコマンドを、出るものに直す。

- [ ] **Step 6: 仕様書に実機の結果を反映する**

`docs/superpowers/specs/2026-11-17-android-haptic-ogg-design.md` の、「挙動の変化」の「音量・マナーモード」の項の末尾の、`マナーモードで振動するかは、…(受け入れ基準 6)。` を、Step 4 の 6 の実測の結果の文(例: `実装後の実機確認(2026-MM-DD、Pixel 8 Pro)で、マナーモード(バイブ)でも〜だった。`)に置き換える。受け入れ基準 1〜5 の結果は、PR の説明に書くので、仕様書には書かない。

- [ ] **Step 7: コミットする**

```bash
git add docs/guide/user-guide.md docs/superpowers/specs/2026-11-17-android-haptic-ogg-design.md
git commit -m "docs: 触覚入りoggの通知音をユーザーガイドに追加し、実機の確認結果を反映(Issue #592)"
```

(Co-Authored-By トレーラーを付ける)

- [ ] **Step 8: 後始末をする**

```bash
S=$(adb devices | awk 'NR>1 && $2=="device"{print $1; exit}')
adb -s $S shell rm -f /sdcard/Download/haptic.ogg /sdcard/Download/plain.ogg /sdcard/Download/broken_haptic.ogg
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status -sb | head -5
```

Expected: 端末のテスト用ファイルが消え、ワーキングツリーがクリーン。アプリの設定で変えた通知音の選択は、元に戻すようユーザーに伝える。

---

## Self-Review(計画を仕様書と突き合わせた結果)

**1. 仕様書の網羅:**
- 判定(§1)→ Task 1。振り分け・設定・失敗時の beep(§2)→ Task 3。プラグイン Rust 側(§3)→ Task 2 Step 1〜3。Kotlin(§4: `MediaPlayer`、`MediaDataSource`、`prepareAsync`、タイムアウト、後勝ち、API 29 未満は reject)→ Task 2 Step 4〜5。フロント変更なし(§5)→ Task 3 Step 3 でバインディング差分なしを確認。ドキュメント(§6)→ Task 4 Step 5。
- テスト(Rust の判定、Kotlin は CI のビルド、実機の受け入れ基準 1〜6)→ Task 1 / Task 2 Step 7〜8 / Task 4 Step 4。
- 「非対応端末の挙動は未確認」→ ユーザーガイドに「端末による」とだけ書く(Task 4 Step 5)。
- 「OS 通知への拡張」「Opus」「デスクトップ」は対象外のまま。Task 1 の Opus のテストで対象外を固定した。

**2. プレースホルダー:** `<<マナーモードの文>>` は、Task 4 Step 4 の 6 の実測で A / B のどちらかに置き換える指示つき(両方の文面を書いてある)。それ以外に TBD / TODO はない。

**3. 型・名前の一貫性:** `has_haptic_channel(&[u8]) -> bool`(Task 1・3)、`play_haptic_audio(&self, &[u8], bool)`(Task 2・3)、Kotlin の `playHapticAudio` / `PlayHapticAudioArgs { data, hapticsEnabled }` と Rust の `PlayHapticAudioRequest { data, haptics_enabled }`(camelCase)が対応している。`PRESET_BEEP` は `commands/sound.rs` の既存定数。

**4. Review Focus:** 1・2 は Task 1 のテスト、3・4 は Task 2 の実装と Task 4 の基準 4・5、5 は Task 4 の基準 2 に対応づけた。
