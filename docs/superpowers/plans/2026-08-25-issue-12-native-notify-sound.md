# Issue #12: 通知音のRustネイティブ再生 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 通知音の再生をフロントエンド(`AudioContext`)からRust側(`rodio`)のネイティブ再生に移し、webviewの自動再生ポリシーに起因する「謎のタイミングで鳴る/鳴らなくなる」不具合(Issue #12)を根本解決する。

**Architecture:** 新しいTauriコマンド`play_notify_sound(choice: String)`をRust側に追加する。`choice`の解釈(プリセットID→埋め込みWAV、`data:`URL→base64デコード)は副作用のない純粋関数`resolve_audio_bytes()`に分離し、実際のデバイス出力はバックグラウンドスレッドで動く`SoundPlayer`(`rodio`の`Mixer`に都度足すだけで多重再生を許容)が担う。フロントエンドは`playNotifySound()`のWeb Audio API実装を丸ごと削除し、全ての呼び出し箇所を新コマンドのinvokeに置き換える。

**Tech Stack:** Rust(`rodio` 0.22, `base64`(既存依存)), TypeScript/Svelte(`tauri-specta`生成バインディング経由)

## Global Constraints

- `rodio`はデフォルトfeatureのまま追加する(wav/mp3/ogg/aac系のsymphoniaデコーダが標準で含まれるため、featureの絞り込みは不要)
- `play_notify_sound`コマンドは失敗時も`Err`を返さない。失敗は`log::warn!`にログを残すのみで、通知フロー全体を止めない(既存JS実装の「音の失敗は無視」方針を踏襲しつつ、Rust側ログで追跡可能にする)
- 対象プラットフォームはデスクトップ+Androidの全て。ただしAndroid実機での動作確認は自動化できないため手動検証項目とする
- コミットメッセージは要約1行のみ(本文・箇条書きなし。Co-Authored-Byトレーラは別途付与される)
- 設計の詳細根拠は `docs/superpowers/specs/2026-08-25-issue-12-native-notify-sound-design.md` を参照

---

### Task 1: 通知音プリセットのWAVアセット生成

現行の`playTone()`/`playPreset()`(Web Audio APIオシレーター合成)と同じパラメータで、4種のプリセット音を事前レンダリングしたWAVとして`src-tauri/assets/sounds/`に用意する。実行時合成はやめ、Rust側は埋め込み済みの音源を再生するだけにする。

**Files:**
- Create: `scripts/render_notify_sounds.py`
- Create: `src-tauri/assets/sounds/beep.wav`
- Create: `src-tauri/assets/sounds/chime.wav`
- Create: `src-tauri/assets/sounds/ping.wav`
- Create: `src-tauri/assets/sounds/pop.wav`

**Interfaces:**
- Produces: `src-tauri/assets/sounds/{beep,chime,ping,pop}.wav` (16bit PCM モノラル, 44100Hz) — Task 2で`include_bytes!`により埋め込む

- [ ] **Step 1: レンダリングスクリプトを書く**

```python
#!/usr/bin/env python3
"""通知音プリセット(beep/chime/ping/pop)のWAVアセットを生成する。

frontend/src/lib/store.svelte.ts の playTone()/playPreset() (Web Audio API
オシレーター合成) と同じパラメータで、事前レンダリングしたWAVを
src-tauri/assets/sounds/ に書き出す(Issue #12: 実行時合成をやめてRust側で
ネイティブ再生する設計に伴う一度きりのアセット生成)。

再生成する場合: python3 scripts/render_notify_sounds.py
"""

import math
import os
import struct
import wave

SAMPLE_RATE = 44100
FLOOR = 0.0001
ATTACK = 0.01


def synth_tone(freq: float, delay: float, dur: float, wave_type: str = "sine", peak: float = 0.15) -> list[float]:
    n_delay = int(delay * SAMPLE_RATE)
    n_dur = int(dur * SAMPLE_RATE)
    n_attack = max(1, min(n_dur, int(ATTACK * SAMPLE_RATE)))
    samples = [0.0] * (n_delay + n_dur)
    for i in range(n_dur):
        t = i / SAMPLE_RATE
        if i < n_attack:
            amp = FLOOR * (peak / FLOOR) ** (i / n_attack)
        else:
            remain = n_dur - n_attack
            frac = (i - n_attack) / remain if remain > 0 else 1.0
            amp = peak * (FLOOR / peak) ** frac
        if wave_type == "sine":
            wave_val = math.sin(2 * math.pi * freq * t)
        else:  # triangle
            wave_val = (2 / math.pi) * math.asin(math.sin(2 * math.pi * freq * t))
        samples[n_delay + i] = amp * wave_val
    return samples


def mix(*tracks: list[float]) -> list[float]:
    length = max(len(t) for t in tracks)
    out = [0.0] * length
    for t in tracks:
        for i, v in enumerate(t):
            out[i] += v
    return [max(-1.0, min(1.0, v)) for v in out]


def write_wav(path: str, samples: list[float]) -> None:
    with wave.open(path, "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SAMPLE_RATE)
        frames = b"".join(struct.pack("<h", int(v * 32767)) for v in samples)
        w.writeframes(frames)


def main() -> None:
    out_dir = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "assets", "sounds")
    os.makedirs(out_dir, exist_ok=True)

    presets = {
        "beep.wav": synth_tone(880, 0, 0.18),
        "chime.wav": mix(synth_tone(660, 0, 0.12), synth_tone(880, 0.1, 0.16)),
        "ping.wav": synth_tone(1300, 0, 0.09, "sine", 0.12),
        "pop.wav": synth_tone(220, 0, 0.09, "triangle", 0.2),
    }
    for name, samples in presets.items():
        write_wav(os.path.join(out_dir, name), samples)
        print(f"wrote {name} ({len(samples)} samples)")


if __name__ == "__main__":
    main()
```

保存先: `scripts/render_notify_sounds.py`

- [ ] **Step 2: スクリプトを実行してWAVを生成する**

Run: `python3 scripts/render_notify_sounds.py`
Expected: `wrote beep.wav (...)` 等4行が出力され、`src-tauri/assets/sounds/`に4ファイルが生成される

- [ ] **Step 3: 生成物を確認する**

Run: `ls -la src-tauri/assets/sounds/`
Expected: `beep.wav`, `chime.wav`, `ping.wav`, `pop.wav` が存在し、いずれも0バイトでない

- [ ] **Step 4: コミット**

```bash
git add scripts/render_notify_sounds.py src-tauri/assets/sounds/
git commit -m "feat: 通知音プリセットのWAVアセットを追加"
```

---

### Task 2: `resolve_audio_bytes` 純粋関数の実装(TDD)

`choice`文字列(プリセットID or `data:`URL)を実際に再生するバイト列に解決する、副作用のない関数を先にテストから実装する。

**Files:**
- Create: `src-tauri/src/commands/sound.rs`
- Modify: `src-tauri/src/commands/mod.rs` — `pub mod sound;` を追加

**Interfaces:**
- Consumes: `src-tauri/assets/sounds/{beep,chime,ping,pop}.wav`(Task 1の成果物、`include_bytes!`で埋め込む)、`crate::error::{Error, Result}`(既存)
- Produces: `pub(crate) fn resolve_audio_bytes(choice: &str) -> Result<std::borrow::Cow<'static, [u8]>>` — Task 4で`play_notify_sound`コマンドから呼ぶ

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/mod.rs`の`pub mod mute;`の下に`pub mod sound;`を1行追加する。

`src-tauri/src/commands/sound.rs`を新規作成し、まずテストのみ書く(この時点では`resolve_audio_bytes`は未定義なのでコンパイルが通らない):

```rust
//! 通知音を鳴らす Tauri コマンド(Issue #12: webview の AudioContext 自動再生ポリシーに
//! 左右されないよう、実際の再生は Rust 側(rodio)で行う)。

#[cfg(test)]
mod tests {
    use super::resolve_audio_bytes;

    #[test]
    fn resolves_known_presets() {
        assert!(!resolve_audio_bytes("beep").unwrap().is_empty());
        assert!(!resolve_audio_bytes("chime").unwrap().is_empty());
        assert!(!resolve_audio_bytes("ping").unwrap().is_empty());
        assert!(!resolve_audio_bytes("pop").unwrap().is_empty());
    }

    #[test]
    fn distinct_presets_have_distinct_bytes() {
        let beep = resolve_audio_bytes("beep").unwrap();
        let chime = resolve_audio_bytes("chime").unwrap();
        assert_ne!(beep.as_ref(), chime.as_ref());
    }

    #[test]
    fn empty_choice_defaults_to_beep() {
        assert_eq!(
            resolve_audio_bytes("").unwrap().as_ref(),
            resolve_audio_bytes("beep").unwrap().as_ref()
        );
    }

    #[test]
    fn unknown_preset_id_falls_back_to_beep() {
        assert_eq!(
            resolve_audio_bytes("not-a-preset").unwrap().as_ref(),
            resolve_audio_bytes("beep").unwrap().as_ref()
        );
    }

    #[test]
    fn decodes_data_url() {
        // "data:audio/wav;base64,aGVsbG8=" -> "hello"
        let got = resolve_audio_bytes("data:audio/wav;base64,aGVsbG8=").unwrap();
        assert_eq!(got.as_ref(), b"hello");
    }

    #[test]
    fn rejects_data_url_without_comma() {
        assert!(resolve_audio_bytes("data:audio/wav;base64").is_err());
    }

    #[test]
    fn rejects_invalid_base64() {
        assert!(resolve_audio_bytes("data:audio/wav;base64,not base64!!").is_err());
    }
}
```

- [ ] **Step 2: テストが失敗する(コンパイルエラーになる)ことを確認する**

Run: `cd src-tauri && cargo test -p tsumugi_lib resolve_audio`
Expected: FAIL — `error[E0425]: cannot find function `resolve_audio_bytes` in this scope`

- [ ] **Step 3: 最小限の実装を書く**

`src-tauri/src/commands/sound.rs`の`#[cfg(test)]`ブロックの**上**に追加する:

```rust
use crate::error::{Error, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::borrow::Cow;

const PRESET_BEEP: &[u8] = include_bytes!("../../assets/sounds/beep.wav");
const PRESET_CHIME: &[u8] = include_bytes!("../../assets/sounds/chime.wav");
const PRESET_PING: &[u8] = include_bytes!("../../assets/sounds/ping.wav");
const PRESET_POP: &[u8] = include_bytes!("../../assets/sounds/pop.wav");

/// choice(プリセットID または data: URL) から実際に再生するバイト列を解決する。
/// 副作用を持たない純粋関数(単体テスト用に分離)。
pub(crate) fn resolve_audio_bytes(choice: &str) -> Result<Cow<'static, [u8]>> {
    if let Some(rest) = choice.strip_prefix("data:") {
        let comma = rest
            .find(',')
            .ok_or_else(|| Error::Invalid(format!("malformed data URL: {choice}")))?;
        let bytes = STANDARD
            .decode(&rest[comma + 1..])
            .map_err(|e| Error::Invalid(format!("failed to decode data URL: {e}")))?;
        return Ok(Cow::Owned(bytes));
    }
    Ok(Cow::Borrowed(match choice {
        "chime" => PRESET_CHIME,
        "ping" => PRESET_PING,
        "pop" => PRESET_POP,
        _ => PRESET_BEEP, // "beep" と空文字(既定)・未知の文字列はここに含む(JS版のdefault分岐と同じ)
    }))
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test -p tsumugi_lib resolve_audio`
Expected: PASS — 7件のテストが全て通る

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/commands/sound.rs src-tauri/src/commands/mod.rs
git commit -m "feat: 通知音choiceをバイト列に解決するresolve_audio_bytesを追加"
```

---

### Task 3: `SoundPlayer`(バックグラウンド再生スレッド)の実装とAppStateへの組み込み

`rodio`依存を追加し、音声出力デバイスを一度だけ開いてバックグラウンドスレッドで保持する`SoundPlayer`を実装する。複数の通知音が重なっても`Mixer`が自動的に重畳再生するため、呼び出しごとにデバイスを開き直さない。

**Files:**
- Modify: `src-tauri/Cargo.toml` — `rodio`を`[dependencies]`に追加
- Create: `src-tauri/src/sound.rs`
- Modify: `src-tauri/src/lib.rs` — `mod sound;`を追加
- Modify: `src-tauri/src/state.rs` — `AppState`に`sound: sound::SoundPlayer`フィールドを追加

**Interfaces:**
- Consumes: なし(このタスクは新規追加のみ)
- Produces: `pub struct SoundPlayer` with `pub fn spawn() -> Self` と `pub fn play(&self, bytes: Vec<u8>)` — Task 4で`AppState.sound.play(...)`として呼ぶ

- [ ] **Step 1: `rodio`依存を追加する**

`src-tauri/Cargo.toml`の`[dependencies]`セクション、`infer = "0.22.0"`の下に1行追加する:

```toml
rodio = "0.22"
```

- [ ] **Step 2: `SoundPlayer`を実装する**

`src-tauri/src/sound.rs`を新規作成する:

```rust
//! 通知音のネイティブ再生。webview の AudioContext 自動再生ポリシーに左右されないよう、
//! Rust 側で rodio 経由で鳴らす(Issue #12)。
//!
//! 出力デバイスは起動時に一度だけ開いてバックグラウンドスレッドに保持する。
//! 再生要求はチャンネル経由で送るだけで即座に返り、実際のデコード/再生は
//! そのスレッド内で行う。複数の通知音が重なっても Mixer が重畳して鳴らすため、
//! 呼び出しごとに出力デバイスを開き直す必要はない。

use rodio::{Decoder, DeviceSinkBuilder};
use std::io::Cursor;
use std::sync::mpsc;

pub struct SoundPlayer {
    tx: mpsc::Sender<Vec<u8>>,
}

impl SoundPlayer {
    /// 再生用バックグラウンドスレッドを起動する。出力デバイスが無い等で開けない
    /// 場合はログを残してスレッドを終了する(以降の`play`はログのみで何も鳴らさない)。
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut handle = match DeviceSinkBuilder::open_default_sink() {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("通知音: 出力デバイスの取得に失敗: {e}");
                    return;
                }
            };
            handle.log_on_drop(false);
            for bytes in rx {
                let cursor = Cursor::new(bytes);
                match Decoder::try_from(cursor) {
                    Ok(source) => handle.mixer().add(source),
                    Err(e) => log::warn!("通知音: デコードに失敗: {e}"),
                }
            }
        });
        Self { tx }
    }

    /// バイト列を非同期に再生する(デコード/再生はバックグラウンドスレッドで行うため即座に返る)。
    pub fn play(&self, bytes: Vec<u8>) {
        if self.tx.send(bytes).is_err() {
            log::warn!("通知音: 再生スレッドが終了しているため再生できません");
        }
    }
}
```

- [ ] **Step 3: `lib.rs`にモジュール登録する**

`src-tauri/src/lib.rs`冒頭の`mod`一覧、`mod session;`の下に1行追加する:

```rust
mod sound;
```

- [ ] **Step 4: `AppState`にフィールドを追加する**

`src-tauri/src/state.rs`の`use`に追加:

```rust
use crate::sound::SoundPlayer;
```

`AppState`構造体定義に1フィールド追加(`gap_fill_in_flight`フィールドの下):

```rust
    /// 通知音のネイティブ再生(Issue #12)。
    pub sound: SoundPlayer,
```

`AppState::new`の`Self { ... }`内、`gap_fill_in_flight: Mutex::new(HashSet::new()),`の下に追加:

```rust
            sound: SoundPlayer::spawn(),
```

- [ ] **Step 5: ビルドが通ることを確認する**

Run: `cd src-tauri && cargo build`
Expected: ビルド成功(警告は無視して良いが、エラーが無いこと)

- [ ] **Step 6: 既存テストが壊れていないことを確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS(既存テスト+Task 2で追加したテストが全て通る)

- [ ] **Step 7: コミット**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/sound.rs src-tauri/src/lib.rs src-tauri/src/state.rs
git commit -m "feat: 通知音をネイティブ再生するSoundPlayerを追加"
```

---

### Task 4: `play_notify_sound`コマンドの登録とTSバインディング再生成

Task 2/3の部品を組み合わせ、実際にフロントエンドから呼べるTauriコマンドとして公開する。

**Files:**
- Modify: `src-tauri/src/commands/sound.rs` — コマンド本体を追加
- Modify: `src-tauri/src/commands/mod.rs` — re-export追加
- Modify: `src-tauri/src/lib.rs` — `specta_builder()`の`collect_commands!`に登録
- Modify: `frontend/src/bindings/tauri.gen.ts` — 自動生成(手で編集しない)

**Interfaces:**
- Consumes: `resolve_audio_bytes()`(Task 2)、`AppState.sound: SoundPlayer`(Task 3)
- Produces: フロントエンドから`commands.playNotifySound(choice: string): Promise<Result<null, Error>>`として呼べるようになる(Task 5で使用)

- [ ] **Step 1: コマンド本体を書く**

`src-tauri/src/commands/sound.rs`の`use`宣言に追加:

```rust
use crate::state::AppState;
use tauri::State;
```

`resolve_audio_bytes`関数の下、`#[cfg(test)]`ブロックの**上**に追加:

```rust
/// 通知音を鳴らす。choice は プリセットID / data URL(カスタム音声)。
/// 失敗しても通知フロー全体を止めないため、常に Ok を返す(失敗はログのみ)。
#[tauri::command]
#[specta::specta]
pub async fn play_notify_sound(state: State<'_, AppState>, choice: String) -> Result<()> {
    match resolve_audio_bytes(&choice) {
        Ok(bytes) => state.sound.play(bytes.into_owned()),
        Err(e) => log::warn!("通知音: 再生対象の解決に失敗: {e}"),
    }
    Ok(())
}
```

- [ ] **Step 2: `commands/mod.rs`にre-exportを追加する**

`src-tauri/src/commands/mod.rs`の末尾、`pub use user::{...}`の下に追加:

```rust
#[allow(unused_imports)]
pub use sound::play_notify_sound;
```

- [ ] **Step 3: `specta_builder()`に登録する**

`src-tauri/src/lib.rs`の`collect_commands![`内、`commands::mute::read_audio_data_url,`の下に1行追加する:

```rust
            commands::sound::play_notify_sound,
```

- [ ] **Step 4: TSバインディングを再生成する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts`に`playNotifySound`関連の型/関数が追加される

- [ ] **Step 5: 生成結果を確認する**

Run: `grep -n "playNotifySound" frontend/src/bindings/tauri.gen.ts`
Expected: `playNotifySound`を呼ぶ関数と型定義が出力される

- [ ] **Step 6: Rust側テスト一式を確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/commands/sound.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: play_notify_soundコマンドを追加してTSバインディングを再生成"
```

---

### Task 5: フロントエンドをネイティブコマンド呼び出しに置き換える

`AudioContext`ベースの実装(`playTone`/`playPreset`/`playNotifySound`/`audioCtx`)を削除し、全呼び出し箇所を`commands.playNotifySound`のinvokeに置き換える。

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:811-816`(`columnNote`ハンドラ内の`wantsSound`発火箇所)
- Modify: `frontend/src/lib/store.svelte.ts:899-904`(`columnNotification`ハンドラ内の`wantsSound`発火箇所)
- Modify: `frontend/src/lib/store.svelte.ts:1831-1878`(`playTone`/`playPreset`/`playNotifySound`/`audioCtx`を削除)
- Modify: `frontend/src/ui/settings/NotifySection.svelte`(試聴ボタン2箇所、import文)

**Interfaces:**
- Consumes: `commands.playNotifySound(choice: string)`(Task 4)、`unwrap`(`frontend/src/lib/ipc.ts`、既存)

- [ ] **Step 1: `columnNote`ハンドラの発火箇所を置き換える**

`frontend/src/lib/store.svelte.ts:816`:

置き換え前:
```typescript
          if (wantsSound) playNotifySound(this.#resolveSoundChoice(tab));
```

置き換え後:
```typescript
          if (wantsSound) void unwrap(commands.playNotifySound(this.#resolveSoundChoice(tab)));
```

- [ ] **Step 2: `columnNotification`ハンドラの発火箇所を置き換える**

`frontend/src/lib/store.svelte.ts:904`(Step 1適用後は行番号がずれるため、`if (wantsSound) playNotifySound(this.#resolveSoundChoice(tab));`という同一パターンの**2箇所目**を対象にする):

置き換え前:
```typescript
          if (wantsSound) playNotifySound(this.#resolveSoundChoice(tab));
```

置き換え後:
```typescript
          if (wantsSound) void unwrap(commands.playNotifySound(this.#resolveSoundChoice(tab)));
```

- [ ] **Step 3: 旧Web Audio実装を削除する**

`frontend/src/lib/store.svelte.ts`から以下のブロックを丸ごと削除する(`// ---- 通知の見出し / 音 ----`のコメントと`notifActionLabel`関数、`NOTIFY_SOUND_PRESETS`定数は残す。削除するのは`playTone`/`playPreset`/`playNotifySound`/`let audioCtx`のみ):

```typescript
let audioCtx: AudioContext | null = null;
function playTone(freq: number, delay: number, dur: number, type: OscillatorType = "sine", peak = 0.15) {
  audioCtx ??= new AudioContext();
  const ctx = audioCtx;
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = type;
  osc.frequency.value = freq;
  gain.gain.value = 0.0001;
  osc.connect(gain).connect(ctx.destination);
  const now = ctx.currentTime + delay;
  gain.gain.exponentialRampToValueAtTime(peak, now + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + dur);
  osc.start(now);
  osc.stop(now + dur + 0.02);
}

function playPreset(preset: string) {
  switch (preset) {
    case "chime":
      playTone(660, 0, 0.12);
      playTone(880, 0.1, 0.16);
      break;
    case "ping":
      playTone(1300, 0, 0.09, "sine", 0.12);
      break;
    case "pop":
      playTone(220, 0, 0.09, "triangle", 0.2);
      break;
    case "beep":
    default:
      playTone(880, 0, 0.18);
      break;
  }
}

/// 通知音を鳴らす。choice は プリセットID / data URL(カスタム音声) / 空文字(既定=beep)。
export function playNotifySound(choice: string) {
  try {
    if (choice.startsWith("data:")) {
      void new Audio(choice).play().catch(() => {});
      return;
    }
    playPreset(choice || "beep");
  } catch {
    // 音の失敗は無視
  }
}
```

- [ ] **Step 4: `NotifySection.svelte`のimportを差し替える**

`frontend/src/ui/settings/NotifySection.svelte:3`:

置き換え前:
```typescript
  import { app, NOTIFY_SOUND_PRESETS, playNotifySound } from "../../lib/store.svelte";
```

置き換え後:
```typescript
  import { app, NOTIFY_SOUND_PRESETS } from "../../lib/store.svelte";
  import { commands, unwrap } from "../../lib/ipc";
```

- [ ] **Step 5: 試聴ボタン2箇所を置き換える**

`frontend/src/ui/settings/NotifySection.svelte:80`:

置き換え前:
```svelte
          <Button type="button" variant="outline" size="sm" onclick={() => playNotifySound(soundChoice)}>試聴</Button>
```

置き換え後:
```svelte
          <Button type="button" variant="outline" size="sm" onclick={() => unwrap(commands.playNotifySound(soundChoice))}>試聴</Button>
```

`frontend/src/ui/settings/NotifySection.svelte:84`:

置き換え前:
```svelte
      <Button type="button" variant="outline" size="sm" onclick={() => playNotifySound(soundMode)}>試聴</Button>
```

置き換え後:
```svelte
      <Button type="button" variant="outline" size="sm" onclick={() => unwrap(commands.playNotifySound(soundMode))}>試聴</Button>
```

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無し(`playNotifySound`未使用importや型不一致が無いこと)

- [ ] **Step 7: フロントエンドテストを実行する**

Run: `cd frontend && pnpm test`
Expected: PASS(既存テストは`invokeMock`が`{status:"ok",data:null}`を返すため、新しい`commands.playNotifySound`呼び出しも追加修正無しで通る)

- [ ] **Step 8: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/ui/settings/NotifySection.svelte
git commit -m "feat: 通知音の再生をplay_notify_soundコマンド呼び出しに置き換え"
```

---

### Task 6: 手動動作確認

自動テストでは検証できない「実際に音が鳴るか」「webviewバックグラウンド時も遅延しないか」を手動で確認する。

**Files:** なし(検証のみ)

**Interfaces:** なし

- [ ] **Step 1: デスクトップで起動する**

Run: `cargo tauri dev`(リポジトリルートから。`src-tauri`の中からは起動しない)

- [ ] **Step 2: 試聴ボタンで4プリセット全てを確認する**

設定→通知→「通知音の種類」を切り替えながら「試聴」ボタンを押し、beep/chime/ping/popそれぞれが鳴ることを目視(耳)確認する。

- [ ] **Step 3: カスタム音声ファイルでも確認する**

「カスタム(音声ファイル)」を選び、任意の音声ファイルを選択して「試聴」を押し、鳴ることを確認する。

- [ ] **Step 4: 実際の通知発火で確認する**

通知をONにしたタブ/通知カラムがある状態で、別アカウント等から実際に通知(メンション等)を受け取り、即座に音が鳴ることを確認する(Issue #12の再現条件だった「ウィンドウを最小化/バックグラウンドにした状態でしばらく置いてから通知を受け取る」パターンも試す)。

- [ ] **Step 5: 起動したdevサーバーを終了する**

確認が終わったら、自分で起動した`cargo tauri dev`プロセスを終了する。

- [ ] **Step 6: Android(可能であれば)**

Android実機/エミュレータがあれば`cd src-tauri && cargo tauri android build --debug --target aarch64`でビルドし、同様に試聴ボタンで確認する。環境が無ければこのステップはスキップして良い(design docに明記済みの既知の検証ギャップ)。
