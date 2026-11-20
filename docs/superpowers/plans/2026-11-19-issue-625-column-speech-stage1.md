# カラム読み上げ(段階1)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 読み上げをオンにしたカラムに届いたライブのノートを、外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)で音声合成して順番に読み上げる(Issue #625 の段階1)。

**Architecture:** フロント(`store.svelte.ts` の `columnNote` リスナー)が発火判定と `mfm-js` による整形を行い、整形済みの文字列を `speak_note` で Rust に渡す。Rust の `tts/` が、上限付きキュー・外部エンジンへの合成・再生を持つ。再生は既存 `SoundPlayer`(rodio)に「読み上げレーン」を足して逐次再生・停止・完了通知を実現する。エンジンは `SpeechEngine` trait で抽象化し、段階2以降の内蔵 CORE を同じ trait で足せるようにする。

**Tech Stack:** Rust(tokio、reqwest、rodio 0.22 の `Player`、async-trait、axum はテストのフェイクサーバーのみ)、Tauri v2 + tauri-specta、Svelte 5(runes)、`mfm-js`、Vitest。

**Spec:** `docs/superpowers/specs/2026-11-19-issue-625-column-speech-design.md`

## Global Constraints

- ブランチは `feat/issue-625-column-speech`(作成済み)。`main` では編集もコミットもしない。
- コミットメッセージは件名1行のみ・Conventional Commits 形式(例 `feat(tts): …`)。本文・箇条書きは書かない(`Co-Authored-By` のトレーラーは別途付く)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告し、再試行しない。
- 設定は JSON ファイル(`SettingsStore` の `SettingsData`)。SQLite の `column_def` は旧データ移行専用なので触らない。マイグレーションは不要で、新しいフィールドには `#[serde(default)]` を付ける。
- `Column.speech` と `SettingsData.speech` を足すので、TS バインディング `frontend/src/bindings/tauri.gen.ts`(git 管理)は手で編集せず、`cd src-tauri && cargo test generates_frontend_bindings` で再生成してコミットする。
- 新しいコマンド・イベントは `lib.rs` の `specta_builder()` に登録する(`tauri::Builder` への登録だけでは TS に出ない)。
- 外部エンジン用の `reqwest::Client` は、接続タイムアウトと全体タイムアウトを必ず設定する(reqwest の既定は無制限)。リダイレクトは追わない。接続先は設定済みの URL だけ。URL のスキームは `http` / `https` のみ。
- 読み上げ設定のうち `SettingsData.speech`(`external_url` を含む)は、エクスポートに含めず、インポートでは現在の値を維持する。`Column.speech` は含める(インポート時に `normalized()` で丸める)。
- 読み上げの失敗(エンジン停止・合成失敗・再生失敗)は、ノートの受信・表示・通知を妨げない(`Err` はログと `SpeechStatus` に残すだけ)。
- フロントの UI は `docs/design/style-guide.md` の既存の目盛り(border-radius / font-size / focus-visible など)だけを使い、一回限りの値(`rounded-[Npx]` など)を作らない。
- 通知カラム(`type: "notifications"`)は読み上げの対象外。起動時・再接続時のギャップ埋め(`columnGapFill`)のノートは読まない。
- Rust のビルド・テストは CLAUDE.md の注意に従う(`llama.cpp` のビルドは並列度 4 まで・`CARGO_TARGET_DIR` を `/tmp` に置かない。初回の全体ビルドは 15〜30 分かかる)。単体テストは `cd src-tauri && cargo test --lib <フィルタ>` で絞って実行する。
- 実アプリの動作確認は Xvfb 越しに行い、`WAYLAND_DISPLAY` を unset する(ユーザーの実画面に漏らさない)。自分で起動した `cargo tauri dev` や Docker コンテナは、タスク完了前に自分で止める(PID 指定で kill する。`pkill` / `killall` は使わない。コンテナは `docker rm -fv <id>`)。
- コード中のコメントは日本語(既存の流儀)。

## Review Focus

spec が暗黙に前提としているが、各タスクの通常のテストでは当たりにくい入力・状態(起きやすい順)。それぞれ、担当タスクにテストを入れてある。

1. **エンジンが応答しない・途中で止まる**: 合成が全体タイムアウトで打ち切られ、キューが詰まらず、ノートの受信・表示は止まらない(Task 3 のタイムアウトのテスト、Task 6 の失敗のテスト)。
2. **巨大・異常な本文**: 数万文字の本文や深くネストした MFM でも、整形が一瞬で終わり、120 文字で切られる。Rust 側は 300 文字を超える入力を拒否する(Task 8、Task 7)。
3. **読み上げる中身が無いノート**: 絵文字・URL・コードだけ、空白だけ、画像だけ、純粋なリノートは `null`(読まない)になり、「さん、」だけが読まれたりしない(Task 8)。
4. **同じノートの二重読み上げ**: 複数カラムに同じノートが届いても、通知音の重複排除の枠を食わず、1回だけ読まれる(Task 4 の重複排除、Task 9 の `speak:` キー)。
5. **読み上げ中に設定を切る・カラムを閉じる**: 全体スイッチをオフにする、カラムの読み上げをオフにする、カラムを閉じると、待ち項目が消え、停止後に合成が終わった音声は再生されない(Task 4、Task 6 の停止のテスト、Task 7 の `close_column` 連動)。

---

## File Structure

新規(Rust):

| ファイル | 責務 |
|---|---|
| `src-tauri/src/domain/speech.rs` | `ColumnSpeech` / `SpeechSettings` / `SpeechEngineKind` の型・検証・丸め |
| `src-tauri/src/tts/mod.rs` | `tts` モジュールの入口と、識別コード(`SPEECH_*`)・`invalid()` |
| `src-tauri/src/tts/engine.rs` | `SpeechEngine` trait、`Voice`、`SynthParams` |
| `src-tauri/src/tts/external.rs` | VOICEVOX 互換 HTTP クライアント(`ExternalEngine`) |
| `src-tauri/src/tts/queue.rs` | 音声と切り離した純粋なキュー(上限・重複排除・カラム単位の削除) |
| `src-tauri/src/tts/manager.rs` | `SpeechManager`(キューと状態)、ワーカーのループ、`SpeechOutput` / `WorkerContext` trait |
| `src-tauri/src/tts/runtime.rs` | 本番の `WorkerContext`(`AppHandle` 経由)と `spawn()` |
| `src-tauri/src/speech_lane.rs` | rodio の `Player` で読み上げを逐次再生・停止・完了通知する小さな部品 |
| `src-tauri/src/commands/speech.rs` | 読み上げの Tauri コマンド |

変更(Rust): `domain/mod.rs`、`domain/column.rs`(`Column.speech`)、`store/settings.rs`(`SettingsData.speech` と取得・更新)、`store/settings_transfer.rs`(インポートで現在値を維持)、`sound.rs`(読み上げレーン)、`state.rs`(`speech` フィールド)、`events.rs`(`SpeechStatusChanged`)、`commands/mod.rs`、`commands/column.rs`(`close_column` 連動)、`lib.rs`(モジュール・コマンド・イベント登録、`spawn`)、`Column` リテラルを構築している既存のファイル(Task 2)。

新規(フロント): `frontend/src/lib/speechText.ts`(+ test)、`frontend/src/ui/settings/SpeechSection.svelte`(+ test)。

変更(フロント): `lib/ipc.ts`、`lib/store.svelte.ts`、`ui/Settings.svelte`、`ui/AddColumnModal.svelte`(+ test)、`TabView` を作っている既存のテスト、`bindings/tauri.gen.ts`(再生成)。

ドキュメント: `docs/guide/user-guide.md`、`CLAUDE.md`(`tts/` の説明)、`docs/superpowers/plans/2026-11-19-issue-625-spike-notes.md`(スパイクの記録)。

---

### Task 1: スパイク(a)(b): エンジンの互換性と Android の平文 HTTP

プロダクトコードは書かない。結果を記録し、残りのタスクの前提を確定する。

**Files:**
- Create: `docs/superpowers/plans/2026-11-19-issue-625-spike-notes.md`

**Interfaces:**
- Consumes: なし
- Produces: (a) `speedScale` / `volumeScale` が `/audio_query` の応答に存在し、`/synthesis` に効くかの確認結果。(b) Android の `usesCleartextTraffic` の現状。Task 3 と Task 10(UI の注意書き)がこの結論を使う。

- [ ] **Step 1: VOICEVOX ENGINE を起動する(a)**

Docker が使えることを確認してから、ループバックにだけ公開して起動する(`-d` でバックグラウンド)。

```bash
docker run -d --name tsumugi-vv-spike -p '127.0.0.1:50021:50021' voicevox/voicevox_engine:cpu-latest
```

起動を待つ(最大 60 秒)。

```bash
for i in $(seq 1 30); do curl -fsS http://127.0.0.1:50021/version && break; sleep 2; done
```

Expected: バージョン文字列が返る。

- [ ] **Step 2: API の流れと `speedScale` / `volumeScale` を確認する(a)**

```bash
D=/tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/70e2eafc-0f1c-4d93-879a-f7f7a2264aa1/scratchpad/spike; mkdir -p $D && cd $D
curl -fsS http://127.0.0.1:50021/speakers | python3 -I -c 'import sys,json; d=json.load(sys.stdin); print(len(d), "speakers"); print(d[0]["name"], d[0]["styles"][:2])'
curl -fsS -X POST --get 'http://127.0.0.1:50021/audio_query' --data-urlencode 'speaker=1' --data-urlencode 'text=こんにちは、読み上げのテストです' -o q.json
python3 -I -c 'import json; q=json.load(open("q.json")); print({k:q[k] for k in ("speedScale","volumeScale") if k in q})'
```

Expected: `speedScale` と `volumeScale` が表示される(両方とも存在する)。

速度を変えて合成し、WAV の長さが変わることを確認する。

```bash
python3 -I - <<'EOF'
import json, urllib.request, wave, io
q = json.load(open("q.json"))
def synth(scale):
    q2 = dict(q); q2["speedScale"] = scale
    req = urllib.request.Request("http://127.0.0.1:50021/synthesis?speaker=1", data=json.dumps(q2).encode(), headers={"Content-Type": "application/json"}, method="POST")
    b = urllib.request.urlopen(req).read()
    w = wave.open(io.BytesIO(b)); return b[:4], w.getnframes() / w.getframerate()
for s in (1.0, 2.0):
    print(s, synth(s))
EOF
```

Expected: 先頭4バイトが `b'RIFF'`。`speedScale=2.0` の長さが `1.0` のおよそ半分になる。

- [ ] **Step 3: AivisSpeech Engine での確認(a。環境がある場合のみ)**

ユーザーの環境に AivisSpeech Engine が起動済みなら、その URL でステップ2と同じ `curl` / Python を実行する(`127.0.0.1:50021` を差し替える)。**URL が無い・起動していない場合は実行せず、記録に「未検証」と書く**(イメージ名やポートを推測で使わない)。

- [ ] **Step 4: Android の平文 HTTP の現状を調べる(b)**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri
grep -rn "usesCleartextTraffic\|networkSecurityConfig\|cleartext" gen/android/app/src/main/AndroidManifest.xml gen/android/app/src/main/res tauri.android.conf.json tauri.conf.json 2>/dev/null
ls gen/android/app/src/main/res/xml 2>/dev/null
```

結果を記録する。判断基準:
- `usesCleartextTraffic="true"` がある、または network security config が LAN への平文を許している → LAN の `http://` の外部エンジンに Android から接続できる見込み。
- 何も設定がない → Android 9(API 28)以降は既定で平文の HTTP が禁止されるので、LAN の `http://` に接続できない見込み。この場合、**実装は止めずに**、UI(Task 10)に「Android では `https://` の URL が必要」と注意書きを出す方針で記録する(マニフェストの変更は、このタスクのスコープ外。必要になったら別 Issue にする。`gen/android/.../generated/` は Tauri が再生成するので編集しない)。

- [ ] **Step 5: 記録を書いて、コンテナを片付ける**

`docs/superpowers/plans/2026-11-19-issue-625-spike-notes.md` に、次の見出しで結果を書く(実際の値で埋める)。

```markdown
# Issue #625 スパイクの記録

## (a) 外部エンジンの互換性
- VOICEVOX ENGINE(イメージ `voicevox/voicevox_engine:cpu-latest`、`/version` の値): `speedScale` / `volumeScale` が audio_query に存在: <はい/いいえ>。speedScale=2.0 で長さが <X.XX> 秒 → <Y.YY> 秒。
- AivisSpeech Engine: <検証した/未検証>(<URL とバージョン、または理由>)。
## (b) Android の平文 HTTP
- 調べたファイルと結果: <…>
- 結論: <LAN の http に接続できる見込み / できない見込み(UI で https を案内する)>
## (c) rodio Player の読み上げレーンが Android で動くか
- 未実施(Task 11 で確認する)。
```

コンテナを片付ける(`-v` を付ける。付けないと匿名ボリュームが孤児で残る)。

```bash
docker rm -fv tsumugi-vv-spike
rm -rf /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/70e2eafc-0f1c-4d93-879a-f7f7a2264aa1/scratchpad/spike
```

- [ ] **Step 6: 結果が前提と違った場合の扱い**

`speedScale` / `volumeScale` が存在しない、または効かない場合は、**ここで止めて**ユーザーに報告する(Task 3 の `ExternalEngine` の設計が変わるため)。前提どおりなら次へ進む。

- [ ] **Step 7: Commit**

```bash
git add docs/superpowers/plans/2026-11-19-issue-625-spike-notes.md
git commit -m "docs(tts): 読み上げのスパイクの記録を追加(Issue #625)"
```

---

### Task 2: データモデルと設定の保存

**Files:**
- Create: `src-tauri/src/domain/speech.rs`
- Modify: `src-tauri/src/domain/mod.rs`、`src-tauri/src/domain/column.rs`
- Modify: `src-tauri/src/store/settings.rs`(`SettingsData.speech`、取得・更新、legacy 移行のリテラル)
- Modify: `src-tauri/src/store/settings_transfer.rs`(インポートで現在値を維持、カラムの丸め)
- Modify(リテラルの追加のみ): `src-tauri/src/attribution.rs`、`src-tauri/src/commands/column.rs`、`src-tauri/src/commands/mute.rs`、`src-tauri/src/commands/settings_transfer.rs`

**Interfaces:**
- Consumes: なし
- Produces(以降のタスクが使う):
  - `domain::ColumnSpeech { enabled: bool, speak_author: bool, queue_limit: u8, voice_id: String }`(`Default`、`normalized(self) -> Self`)
  - `domain::SpeechSettings { enabled: bool, engine: SpeechEngineKind, external_url: String, default_voice_id: String, speed_scale: f32, volume: f32 }`(`Default`、`validate(&self) -> Result<(), String>`)
  - `domain::SpeechEngineKind::External`
  - `domain::speech::parse_base_url(&str) -> Result<url::Url, String>`
  - `domain::speech::{QUEUE_LIMIT_MIN, QUEUE_LIMIT_MAX, QUEUE_LIMIT_DEFAULT: u8}`、`SPEED_MIN/MAX`、`VOLUME_MIN/MAX: f32`、`VOICE_ID_MAX_LEN: usize`
  - `SettingsStore::load_speech(&self) -> SpeechSettings`
  - `SettingsStore::update_speech(&self, f: impl FnOnce(&mut SpeechSettings)) -> Result<SpeechSettings>`
  - `SettingsStore::load_column(&self, column_id: &str) -> Option<Column>`
  - `SettingsStore::set_column_speech(&self, column_id: &str, speech: ColumnSpeech) -> Result<bool>`(カラムが無ければ `Ok(false)`)

- [ ] **Step 1: 型のテストを書く(失敗させる)**

`src-tauri/src/domain/speech.rs` を作り、まずテストだけを書く。

```rust
//! 読み上げ(Issue #625)の設定の型と検証。
//! カラム単位の `ColumnSpeech` は `Column.speech` に、全体の `SpeechSettings` は `SettingsData.speech` に置く。

use serde::{Deserialize, Serialize};
use specta::Type;

pub const QUEUE_LIMIT_MIN: u8 = 1;
pub const QUEUE_LIMIT_MAX: u8 = 20;
pub const QUEUE_LIMIT_DEFAULT: u8 = 5;
pub const SPEED_MIN: f32 = 0.5;
pub const SPEED_MAX: f32 = 2.0;
pub const VOLUME_MIN: f32 = 0.1;
pub const VOLUME_MAX: f32 = 2.0;
/// 声の ID の最大長(エンジン固有の不透明な文字列。VOICEVOX ではスタイルの数値 ID)。
pub const VOICE_ID_MAX_LEN: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_speech_default_is_off_with_author_and_limit_5() {
        let s = ColumnSpeech::default();
        assert!(!s.enabled);
        assert!(s.speak_author);
        assert_eq!(s.queue_limit, QUEUE_LIMIT_DEFAULT);
        assert_eq!(s.voice_id, "");
    }

    #[test]
    fn column_speech_missing_fields_fall_back_to_defaults() {
        let s: ColumnSpeech = serde_json::from_str("{}").unwrap();
        assert_eq!(s, ColumnSpeech::default());
    }

    #[test]
    fn column_speech_normalized_clamps_queue_limit_and_drops_overlong_voice_id() {
        let s = ColumnSpeech { queue_limit: 0, voice_id: "x".repeat(VOICE_ID_MAX_LEN + 1), ..ColumnSpeech::default() }.normalized();
        assert_eq!(s.queue_limit, QUEUE_LIMIT_MIN);
        assert_eq!(s.voice_id, "");
        let s = ColumnSpeech { queue_limit: 200, voice_id: "12".into(), ..ColumnSpeech::default() }.normalized();
        assert_eq!(s.queue_limit, QUEUE_LIMIT_MAX);
        assert_eq!(s.voice_id, "12");
    }

    #[test]
    fn speech_settings_default_is_off_and_unconfigured() {
        let s = SpeechSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.engine, SpeechEngineKind::External);
        assert_eq!(s.external_url, "");
        assert_eq!(s.speed_scale, 1.0);
        assert_eq!(s.volume, 1.0);
        assert!(s.validate().is_ok(), "未設定(URL が空)は有効な設定");
    }

    #[test]
    fn speech_settings_missing_fields_fall_back_to_defaults() {
        let s: SpeechSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, SpeechSettings::default());
    }

    #[test]
    fn url_must_be_http_or_https_with_a_host_and_no_credentials() {
        let ok = |u: &str| SpeechSettings { external_url: u.into(), ..Default::default() }.validate();
        assert!(ok("http://192.168.1.10:50021").is_ok());
        assert!(ok("https://tts.example.com/").is_ok());
        assert!(ok("  http://127.0.0.1:50021  ").is_ok(), "前後の空白は許す");
        assert!(ok("ftp://example.com").is_err());
        assert!(ok("file:///etc/passwd").is_err());
        assert!(ok("javascript:alert(1)").is_err());
        assert!(ok("not a url").is_err());
        assert!(ok("http://user:pass@example.com").is_err());
    }

    #[test]
    fn speed_volume_and_voice_id_ranges_are_enforced() {
        let with = |f: &dyn Fn(&mut SpeechSettings)| {
            let mut s = SpeechSettings::default();
            f(&mut s);
            s.validate()
        };
        assert!(with(&|s| s.speed_scale = 0.4).is_err());
        assert!(with(&|s| s.speed_scale = 2.1).is_err());
        assert!(with(&|s| s.speed_scale = f32::NAN).is_err());
        assert!(with(&|s| s.speed_scale = 2.0).is_ok());
        assert!(with(&|s| s.volume = 0.0).is_err());
        assert!(with(&|s| s.volume = 2.5).is_err());
        assert!(with(&|s| s.default_voice_id = "x".repeat(VOICE_ID_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn parse_base_url_trims_and_strips_trailing_slashes() {
        assert_eq!(parse_base_url(" http://h:1/ ").unwrap().as_str(), "http://h:1/");
        assert!(parse_base_url("").is_err());
    }
}
```

- [ ] **Step 2: モジュールを登録し、失敗を確認する**

`src-tauri/src/domain/mod.rs` の `mod share;` の次の行に `pub mod speech;` を足し(`domain::speech::parse_base_url` や `QUEUE_LIMIT_MAX` を他のモジュールから参照するので、`mod` ではなく `pub mod`)、`pub use share::…` の群の末尾に次を足す(`pub use` の並びは `notify` の行を参考に、既存の末尾へ)。

```rust
pub use speech::{ColumnSpeech, SpeechEngineKind, SpeechSettings};
```

Run: `cd src-tauri && cargo test --lib domain::speech`
Expected: FAIL(`ColumnSpeech` などが未定義でコンパイルエラー)。

- [ ] **Step 3: 型を実装する**

`domain/speech.rs` の `VOICE_ID_MAX_LEN` の定義の後(`#[cfg(test)]` の前)に追加する。

```rust
fn default_true() -> bool {
    true
}
fn default_queue_limit() -> u8 {
    QUEUE_LIMIT_DEFAULT
}
fn default_one() -> f32 {
    1.0
}

/// カラム(タブ)単位の読み上げ設定。`Column.speech`。旧い設定ファイルには無いので、既定値(オフ)で読む。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSpeech {
    /// このカラムを読み上げる。
    #[serde(default)]
    pub enabled: bool,
    /// 投稿者名(`{表示名}さん、`)を前置きする。
    #[serde(default = "default_true")]
    pub speak_author: bool,
    /// このカラムの待ち件数の上限。超えたら、このカラムの最古の待ち項目を捨てる。
    #[serde(default = "default_queue_limit")]
    pub queue_limit: u8,
    /// このカラムの声。空ならグローバルの既定の声を継承する。
    #[serde(default)]
    pub voice_id: String,
}

impl Default for ColumnSpeech {
    fn default() -> Self {
        Self { enabled: false, speak_author: true, queue_limit: QUEUE_LIMIT_DEFAULT, voice_id: String::new() }
    }
}

impl ColumnSpeech {
    /// 保存・インポート前に範囲へ丸める。長すぎる声の ID は(切り詰めると別の ID になるので)空に戻す。
    pub fn normalized(mut self) -> Self {
        self.queue_limit = self.queue_limit.clamp(QUEUE_LIMIT_MIN, QUEUE_LIMIT_MAX);
        if self.voice_id.chars().count() > VOICE_ID_MAX_LEN {
            self.voice_id = String::new();
        }
        self
    }
}

/// 読み上げに使うエンジンの種類。段階2で `VoicevoxCore`(内蔵)を足す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SpeechEngineKind {
    /// 外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)に HTTP で接続する。
    #[default]
    External,
}

/// 全体の読み上げ設定。`SettingsData.speech`。**エクスポートには含めない**。インポートでは現在の値を維持する。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSettings {
    /// 全体のマスタースイッチ。
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub engine: SpeechEngineKind,
    /// 外部エンジンの URL(例 `http://192.168.1.10:50021`)。空は未設定。
    #[serde(default)]
    pub external_url: String,
    /// 既定の声(エンジン固有の ID)。
    #[serde(default)]
    pub default_voice_id: String,
    #[serde(default = "default_one")]
    pub speed_scale: f32,
    #[serde(default = "default_one")]
    pub volume: f32,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: SpeechEngineKind::External,
            external_url: String::new(),
            default_voice_id: String::new(),
            speed_scale: 1.0,
            volume: 1.0,
        }
    }
}

/// 外部エンジンの URL を検証して返す。`http` / `https` のみ・ホストあり・認証情報なし。
/// 末尾のスラッシュは(パスの連結を単純にするため)呼び出し側で揃える。
pub fn parse_base_url(raw: &str) -> Result<url::Url, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("URL が空です".into());
    }
    let u = url::Url::parse(raw).map_err(|_| "URL の形式が正しくありません".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("URL は http:// か https:// で始めてください".into());
    }
    if u.host_str().is_none() {
        return Err("URL にホストがありません".into());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("URL にユーザー名・パスワードは含められません".into());
    }
    Ok(u)
}

impl SpeechSettings {
    /// 保存してよい値か。URL が空(未設定)は有効。範囲外・不正な URL は理由つきで拒否する。
    pub fn validate(&self) -> Result<(), String> {
        if !self.external_url.trim().is_empty() {
            parse_base_url(&self.external_url)?;
        }
        // NaN は範囲に入らないので、ここで弾かれる。
        if !(SPEED_MIN..=SPEED_MAX).contains(&self.speed_scale) {
            return Err(format!("話速は {SPEED_MIN}〜{SPEED_MAX} の範囲で指定してください"));
        }
        if !(VOLUME_MIN..=VOLUME_MAX).contains(&self.volume) {
            return Err(format!("音量は {VOLUME_MIN}〜{VOLUME_MAX} の範囲で指定してください"));
        }
        if self.default_voice_id.chars().count() > VOICE_ID_MAX_LEN {
            return Err("声の ID が長すぎます".into());
        }
        Ok(())
    }
}
```

Run: `cd src-tauri && cargo test --lib domain::speech`
Expected: PASS(7件)。

- [ ] **Step 4: `Column.speech` と `SettingsData.speech` を足す**

`src-tauri/src/domain/column.rs` の `Column` の `title` フィールドの後に追加する。

```rust
    /// このタブの読み上げ設定(Issue #625)。旧い設定ファイルには無いので既定値(オフ)で読む。
    #[serde(default)]
    pub speech: super::speech::ColumnSpeech,
```

`src-tauri/src/store/settings.rs` の `SettingsData` の `local_model_license_version` の後に追加する。

```rust
    /// 読み上げの全体設定(Issue #625)。**エクスポートには含めない**。インポートでは現在の値を維持する
    /// (ファイルで勝手に接続先が変わらないようにするため)。
    #[serde(default)]
    pub(crate) speech: crate::domain::SpeechSettings,
```

- [ ] **Step 5: 既存の `Column` リテラルを更新する**

`notify_sound_choice: String::new(),` の直後に `speech: Default::default(),` を足す(1行・複数行どちらの書き方にも効く)。

```bash
cd src-tauri && perl -0pi -e 's/(notify_sound_choice: String::new\(\),)/$1 speech: Default::default(),/g' \
  src/attribution.rs src/commands/column.rs src/commands/mute.rs src/commands/settings_transfer.rs \
  src/store/settings.rs src/store/settings_transfer.rs
```

`src-tauri/src/store/settings.rs` の legacy 移行(`migrate_from_legacy_sqlite`)の `columns.push(Column { … })` は省略記法(`notify_sound_choice,`)なので、手で足す。

```rust
            notify_sound_choice,
            group_id,
            title,
            speech: Default::default(),
        });
```

さらに、`SettingsData { … }` をリテラルで作っている箇所(`settings.rs` のテスト内の `mcp: Default::default(),` の近く、`store/settings_transfer.rs` の `plan_import` の末尾)に `speech` を足す。`settings.rs` 側:

```rust
            mcp: Default::default(),
            speech: Default::default(),
```

`plan_import` の末尾(`ImportPlan` を作る直前の `SettingsData { … }`)は、次のとおり。**インポートでは現在の値を維持する**。

```rust
        local_model_license_version: current.local_model_license_version.clone(),
        speech: current.speech.clone(),
    };
```

同じ関数のカラムのループ(`let mut c = c.clone(); c.account_id = local_id.clone();` の直後)に、丸めを足す。

```rust
        let mut c = c.clone();
        c.account_id = local_id.clone();
        c.speech = c.speech.normalized();
        columns.push(c);
```

Run: `cd src-tauri && cargo check --tests 2>&1 | tail -30`
Expected: エラーなし。`missing field speech` が残っていれば、その箇所に `speech: Default::default(),` を足す。

- [ ] **Step 6: ストアのテストを書く(失敗させる)**

`src-tauri/src/store/settings.rs` のテストモジュール(`fn column(id: &str, account_id: &str, ord: i32) -> Column` の近く)に追加する。

```rust
    #[test]
    fn old_settings_file_without_speech_loads_with_speech_off() {
        let d: SettingsData = serde_json::from_str("{}").unwrap();
        assert_eq!(d.speech, crate::domain::SpeechSettings::default());
        let c: Column = serde_json::from_str(
            r#"{"id":"c","accountId":"a","kind":{"type":"home"},"order":0,
                "filter":{"kind":"keywords","value":[]},"notifySound":false,"notifyDesktop":false,
                "notifySoundChoice":"","groupId":"g","title":null}"#,
        )
        .unwrap();
        assert_eq!(c.speech, crate::domain::ColumnSpeech::default());
    }

    #[test]
    fn update_speech_returns_and_keeps_the_new_value() {
        let s = SettingsStore::new_in_memory();
        let got = s.update_speech(|v| {
            v.enabled = true;
            v.external_url = "http://h:1".into();
        })
        .unwrap();
        assert!(got.enabled);
        assert_eq!(s.load_speech().external_url, "http://h:1");
    }

    #[test]
    fn set_column_speech_updates_only_that_column_and_reports_missing() {
        let s = SettingsStore::new_in_memory();
        s.upsert_column(&column("c1", "a1", 0)).unwrap();
        s.upsert_column(&column("c2", "a1", 1)).unwrap();
        let on = crate::domain::ColumnSpeech { enabled: true, queue_limit: 3, ..Default::default() };
        assert!(s.set_column_speech("c1", on.clone()).unwrap());
        assert_eq!(s.load_column("c1").unwrap().speech, on);
        assert_eq!(s.load_column("c2").unwrap().speech, crate::domain::ColumnSpeech::default());
        assert!(!s.set_column_speech("nope", on).unwrap(), "存在しないカラムは false");
        assert!(s.load_column("nope").is_none());
    }
```

`src-tauri/src/store/settings_transfer.rs` のテストモジュール(`import_keeps_the_current_local_model_license_version` の次)に追加する。

```rust
    #[test]
    fn import_keeps_the_current_speech_settings_and_export_omits_them() {
        let mut current = current_with_local_account();
        current.speech.enabled = true;
        current.speech.external_url = "http://192.168.1.10:50021".into();
        let plan = plan_import(&current, file_with(empty_data())).unwrap();
        assert!(plan.data.speech.enabled);
        assert_eq!(plan.data.speech.external_url, "http://192.168.1.10:50021");

        // 現在オフなら、ファイルに何があってもオフのまま・URL は空のまま。
        let current = current_with_local_account();
        let plan = plan_import(&current, file_with(empty_data())).unwrap();
        assert!(!plan.data.speech.enabled);
        assert_eq!(plan.data.speech.external_url, "");

        let mut d = settings_data();
        d.speech.external_url = "http://192.168.1.10:50021".into();
        let json = serde_json::to_string(&build_export(&d, "1.2.3", "2026-11-19T00:00:00Z")).unwrap();
        assert!(!json.contains("192.168.1.10"), "エンジンの URL はエクスポートに含めない: {json}");
    }

    #[test]
    fn import_clamps_the_column_speech_but_keeps_what_the_file_says() {
        let current = current_with_local_account();
        let mut data = empty_data();
        data.accounts = vec![account("file1", "misskey.io", "u1")];
        data.groups = vec![group("g1", 0)];
        let mut c = column("c1", "file1", "g1");
        c.speech = crate::domain::ColumnSpeech { enabled: true, speak_author: false, queue_limit: 200, voice_id: "7".into() };
        data.columns = vec![c];
        data.ui.default_account_id = "file1".into();

        let plan = plan_import(&current, file_with(data)).unwrap();

        let s = &plan.data.columns[0].speech;
        assert!(s.enabled);
        assert!(!s.speak_author);
        assert_eq!(s.queue_limit, crate::domain::speech::QUEUE_LIMIT_MAX);
        assert_eq!(s.voice_id, "7");
    }
```

Run: `cd src-tauri && cargo test --lib store::settings`
Expected: FAIL(`load_speech` などが未定義)。

- [ ] **Step 7: ストアの取得・更新を実装する**

`src-tauri/src/store/settings.rs` の `update_mcp` の後(`load_mcp` / `update_mcp` が並んでいる所の直後)に追加する。

```rust
    /// 読み上げの全体設定(Issue #625)。
    pub fn load_speech(&self) -> crate::domain::SpeechSettings {
        self.data.lock().unwrap().speech.clone()
    }

    /// 読み上げの全体設定を更新して保存し、新しい値を返す。
    pub fn update_speech(
        &self,
        f: impl FnOnce(&mut crate::domain::SpeechSettings),
    ) -> Result<crate::domain::SpeechSettings> {
        let mut guard = self.data.lock().unwrap();
        f(&mut guard.speech);
        self.save(&guard)?;
        Ok(guard.speech.clone())
    }

    /// 1つのカラムを返す(無ければ `None`)。
    pub fn load_column(&self, column_id: &str) -> Option<Column> {
        self.data.lock().unwrap().columns.iter().find(|c| c.id == column_id).cloned()
    }

    /// カラムの読み上げ設定を更新する。カラムが無ければ何もせず `Ok(false)`。
    pub fn set_column_speech(&self, column_id: &str, speech: crate::domain::ColumnSpeech) -> Result<bool> {
        let mut guard = self.data.lock().unwrap();
        let Some(c) = guard.columns.iter_mut().find(|x| x.id == column_id) else {
            return Ok(false);
        };
        c.speech = speech;
        self.save(&guard)?;
        Ok(true)
    }
```

Run: `cd src-tauri && cargo test --lib store::settings && cargo test --lib domain::speech`
Expected: PASS。

- [ ] **Step 8: 全体が壊れていないか確認して Commit**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -15`
Expected: 既存のテストを含めて PASS(`#[ignore]` は除く)。

```bash
git add src-tauri/src
git commit -m "feat(tts): 読み上げの設定の型とストアを追加(Issue #625)"
```

---

### Task 3: エンジン抽象と外部エンジン(VOICEVOX 互換クライアント)

**Files:**
- Create: `src-tauri/src/tts/mod.rs`、`src-tauri/src/tts/engine.rs`、`src-tauri/src/tts/external.rs`
- Modify: `src-tauri/src/lib.rs`(`mod tts;`)

**Interfaces:**
- Consumes: `domain::speech::parse_base_url`(Task 2)、`crate::state::USER_AGENT`、`crate::error::{Error, Result}`
- Produces:
  - `tts::{ERR_NOT_CONFIGURED, ERR_UNREACHABLE, ERR_BAD_RESPONSE, ERR_NO_VOICE, ERR_TEXT_TOO_LONG}: &str`、`tts::invalid(code: impl Into<String>) -> Error`
  - `tts::engine::SpeechEngine`(`#[async_trait]`: `async fn voices(&self) -> Result<Vec<Voice>>`、`async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>>`)
  - `tts::engine::Voice { id: String, label: String }`(`Serialize` + `Type`、`camelCase`)、`tts::engine::SynthParams { speed_scale: f32, volume: f32 }`(`Copy`)
  - `tts::external::ExternalEngine::new(base_url: &str) -> Result<Self>`、`ExternalEngine::with_timeouts(base_url: &str, connect: Duration, total: Duration) -> Result<Self>`

- [ ] **Step 1: モジュールの骨組みを作る**

`src-tauri/src/lib.rs` の `mod translate;` の次の行に `mod tts;` を足す。

`src-tauri/src/tts/mod.rs`:

```rust
//! 読み上げ(Issue #625)。エンジン(`engine` / `external`)・キュー(`queue`)・ワーカー(`manager` / `runtime`)。
//! 再生は `crate::sound::SoundPlayer` の読み上げレーンが担う。Tauri への依存は `runtime` と
//! `commands::speech` に閉じる(`translate/` と同じ流儀)。

pub mod engine;
pub mod external;
pub mod manager;
pub mod queue;
pub mod runtime;

use crate::error::Error;

/// エンジンの URL が未設定。
pub const ERR_NOT_CONFIGURED: &str = "SPEECH_ENGINE_NOT_CONFIGURED";
/// エンジンに接続できない・タイムアウトした。
pub const ERR_UNREACHABLE: &str = "SPEECH_ENGINE_UNREACHABLE";
/// エンジンの応答が想定外(HTTP エラー・WAV でない・JSON の形が違う)。
pub const ERR_BAD_RESPONSE: &str = "SPEECH_ENGINE_BAD_RESPONSE";
/// 声が選ばれていない(カラムの声も既定の声も空)。
pub const ERR_NO_VOICE: &str = "SPEECH_NO_VOICE";
/// 読み上げる文が長すぎる。
pub const ERR_TEXT_TOO_LONG: &str = "SPEECH_TEXT_TOO_LONG";

/// 識別コードつきの `Error::Invalid`(翻訳の `LOCAL_*` と同じ流儀)。
pub fn invalid(code: impl Into<String>) -> Error {
    Error::Invalid(code.into())
}
```

`manager` / `queue` / `runtime` は後のタスクで作る。**このタスクの間はコンパイルが通るよう**、3つの空ファイルを作っておく。

```bash
cd src-tauri/src/tts && printf '//! Task 6 で実装する。\n' > manager.rs && printf '//! Task 4 で実装する。\n' > queue.rs && printf '//! Task 7 で実装する。\n' > runtime.rs
```

- [ ] **Step 2: trait と型を書く**

`src-tauri/src/tts/engine.rs`:

```rust
//! 音声合成エンジンの境界(Issue #625)。実体は `external.rs`(外部の VOICEVOX 互換エンジン)で、
//! 段階2で内蔵 CORE を同じ trait で足す。テストでは偽の実装に差し替える。

use crate::error::Result;
use async_trait::async_trait;
use serde::Serialize;
use specta::Type;

/// 選べる声。`id` はエンジン固有の不透明な文字列(VOICEVOX ではスタイルの数値 ID)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    /// 表示名(例 `ずんだもん（ノーマル）`)。
    pub label: String,
}

/// 合成のパラメーター。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthParams {
    pub speed_scale: f32,
    pub volume: f32,
}

#[async_trait]
pub trait SpeechEngine: Send + Sync + 'static {
    /// 選べる声の一覧(設定画面用)。
    async fn voices(&self) -> Result<Vec<Voice>>;
    /// `text` を `voice_id` の声で合成し、WAV のバイト列を返す。
    async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>>;
}
```

- [ ] **Step 3: 外部エンジンのテストを書く(失敗させる)**

`src-tauri/src/tts/external.rs`(まずテストだけ。`axum` はデスクトップ限定の依存なので、テストは `#[cfg(all(test, desktop))]`)。

```rust
//! 外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)への HTTP クライアント(Issue #625)。
//! 使う API は `/speakers`・`/audio_query`・`/synthesis`(VOICEVOX ENGINE の README で確認済み)。

#[cfg(all(test, desktop))]
mod tests {
    use super::*;
    use crate::tts::{ERR_BAD_RESPONSE, ERR_UNREACHABLE};
    use axum::extract::{Query, State};
    use axum::http::{header, StatusCode};
    use axum::response::{IntoResponse, Redirect};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct Seen {
        audio_query: Arc<Mutex<Vec<HashMap<String, String>>>>,
        synthesis: Arc<Mutex<Vec<(HashMap<String, String>, Value)>>>,
    }

    fn wav() -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"WAVEfmt ");
        v
    }

    /// フェイクのエンジンを 127.0.0.1 の空きポートで起動し、`http://127.0.0.1:<port>` を返す。
    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn engine_app(seen: Seen) -> Router {
        Router::new()
            .route(
                "/speakers",
                get(|| async {
                    Json(json!([
                        {"name": "ずんだもん", "speaker_uuid": "u1", "styles": [{"name": "ノーマル", "id": 3}, {"name": "あまあま", "id": 1}]},
                        {"name": "四国めたん", "speaker_uuid": "u2", "styles": [{"name": "ノーマル", "id": 2}]}
                    ]))
                }),
            )
            .route(
                "/audio_query",
                post(|State(s): State<Seen>, Query(q): Query<HashMap<String, String>>| async move {
                    s.audio_query.lock().unwrap().push(q);
                    Json(json!({"accent_phrases": [], "speedScale": 1.0, "volumeScale": 1.0, "pitchScale": 0.0}))
                }),
            )
            .route(
                "/synthesis",
                post(|State(s): State<Seen>, Query(q): Query<HashMap<String, String>>, Json(body): Json<Value>| async move {
                    s.synthesis.lock().unwrap().push((q, body));
                    ([(header::CONTENT_TYPE, "audio/wav")], wav())
                }),
            )
            .with_state(seen)
    }

    #[tokio::test]
    async fn voices_flattens_speakers_and_styles_into_labelled_ids() {
        let url = serve(engine_app(Seen::default())).await;
        let e = ExternalEngine::new(&url).unwrap();
        let v = e.voices().await.unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], Voice { id: "3".into(), label: "ずんだもん（ノーマル）".into() });
        assert_eq!(v[1].id, "1");
        assert_eq!(v[2], Voice { id: "2".into(), label: "四国めたん（ノーマル）".into() });
    }

    #[tokio::test]
    async fn synthesize_sends_text_and_speaker_then_applies_speed_and_volume() {
        let seen = Seen::default();
        let url = serve(engine_app(seen.clone())).await;
        let e = ExternalEngine::new(&format!("{url}/")).unwrap(); // 末尾スラッシュがあっても動く
        let out = e
            .synthesize("こんにちは&テスト", "3", &SynthParams { speed_scale: 1.5, volume: 0.8 })
            .await
            .unwrap();
        assert!(out.starts_with(b"RIFF"));

        let aq = seen.audio_query.lock().unwrap();
        assert_eq!(aq[0].get("speaker").map(String::as_str), Some("3"));
        assert_eq!(aq[0].get("text").map(String::as_str), Some("こんにちは&テスト"), "記号を含む本文が壊れずに届く");

        let sy = seen.synthesis.lock().unwrap();
        assert_eq!(sy[0].0.get("speaker").map(String::as_str), Some("3"));
        assert_eq!(sy[0].1["speedScale"], json!(1.5));
        assert_eq!(sy[0].1["volumeScale"], json!(0.8f32));
        assert_eq!(sy[0].1["pitchScale"], json!(0.0), "上書きしない項目はそのまま渡す");
    }

    #[tokio::test]
    async fn an_http_error_is_a_bad_response() {
        let app = Router::new().route("/audio_query", post(|| async { StatusCode::INTERNAL_SERVER_ERROR }));
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap();
        let err = e.synthesize("a", "1", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap_err();
        assert!(err.to_string().contains(ERR_BAD_RESPONSE), "{err}");
    }

    #[tokio::test]
    async fn a_non_wav_body_is_a_bad_response() {
        let app = engine_app(Seen::default()).route("/synthesis", post(|| async { "not a wav" }));
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap();
        let err = e.synthesize("a", "1", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap_err();
        assert!(err.to_string().contains(ERR_BAD_RESPONSE), "{err}");
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let app = Router::new()
            .route("/speakers", get(|| async { Redirect::temporary("http://127.0.0.1:1/") }));
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap();
        let err = e.voices().await.unwrap_err();
        assert!(err.to_string().contains(ERR_BAD_RESPONSE), "リダイレクト先へは行かず、応答の異常として扱う: {err}");
    }

    #[tokio::test]
    async fn an_unreachable_engine_is_reported_as_unreachable() {
        // 一度開いて閉じたポート = 誰も待ち受けていない。
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let e = ExternalEngine::new(&format!("http://127.0.0.1:{port}")).unwrap();
        let err = e.voices().await.unwrap_err();
        assert!(err.to_string().contains(ERR_UNREACHABLE), "{err}");
    }

    #[tokio::test]
    async fn a_hanging_engine_times_out_instead_of_blocking_forever() {
        let app = Router::new().route(
            "/speakers",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Json(json!([]))
            }),
        );
        let url = serve(app).await;
        let e = ExternalEngine::with_timeouts(&url, Duration::from_millis(200), Duration::from_millis(300)).unwrap();
        let started = std::time::Instant::now();
        let err = e.voices().await.unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5), "タイムアウトで返る: {:?}", started.elapsed());
        assert!(err.to_string().contains(ERR_UNREACHABLE), "{err}");
    }

    #[test]
    fn new_rejects_a_bad_url() {
        assert!(ExternalEngine::new("").is_err());
        assert!(ExternalEngine::new("ftp://example.com").is_err());
        assert!(ExternalEngine::new("http://user:pw@example.com").is_err());
    }
}
```

Run: `cd src-tauri && cargo test --lib tts::external`
Expected: FAIL(`ExternalEngine` が未定義)。

- [ ] **Step 4: クライアントを実装する**

`src-tauri/src/tts/external.rs` の先頭(コメントの直後、`#[cfg(all(test, desktop))]` の前)に追加する。

```rust
use super::engine::{SpeechEngine, SynthParams, Voice};
use super::{invalid, ERR_BAD_RESPONSE, ERR_UNREACHABLE};
use crate::domain::speech::parse_base_url;
use crate::error::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// 接続のタイムアウト。LAN 内の HomeLab を想定した短めの値。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 1リクエスト全体のタイムアウト。長めの文の合成が間に合う程度。
const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ExternalEngine {
    client: reqwest::Client,
    /// 末尾のスラッシュを除いた基準 URL(例 `http://192.168.1.10:50021`)。
    base: String,
}

impl ExternalEngine {
    pub fn new(base_url: &str) -> Result<Self> {
        Self::with_timeouts(base_url, CONNECT_TIMEOUT, TOTAL_TIMEOUT)
    }

    /// タイムアウトを指定して作る(テスト用に公開。本番は `new`)。
    pub fn with_timeouts(base_url: &str, connect: Duration, total: Duration) -> Result<Self> {
        let u = parse_base_url(base_url).map_err(|m| crate::error::Error::Invalid(m))?;
        // reqwest の既定は無制限なので、必ずタイムアウトを付ける。リダイレクトは追わない
        // (設定した URL 以外へ接続しないため)。
        let client = reqwest::Client::builder()
            .user_agent(crate::state::USER_AGENT)
            .connect_timeout(connect)
            .timeout(total)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| crate::error::Error::Invalid(format!("HTTP クライアントを作れません: {e}")))?;
        Ok(Self { client, base: u.as_str().trim_end_matches('/').to_string() })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }
}

/// 通信の失敗(接続不可・タイムアウト)を識別コードつきの `Invalid` にする。
fn unreachable(e: reqwest::Error) -> crate::error::Error {
    invalid(format!("{ERR_UNREACHABLE}: {e}"))
}

fn bad_response(detail: impl std::fmt::Display) -> crate::error::Error {
    invalid(format!("{ERR_BAD_RESPONSE}: {detail}"))
}

#[derive(Deserialize)]
struct SpeakerDto {
    name: String,
    styles: Vec<StyleDto>,
}

#[derive(Deserialize)]
struct StyleDto {
    name: String,
    id: serde_json::Number,
}

#[async_trait]
impl SpeechEngine for ExternalEngine {
    async fn voices(&self) -> Result<Vec<Voice>> {
        let resp = self.client.get(self.url("speakers")).send().await.map_err(unreachable)?;
        if !resp.status().is_success() {
            return Err(bad_response(format!("HTTP {}", resp.status())));
        }
        let speakers: Vec<SpeakerDto> = resp.json().await.map_err(|e| bad_response(e))?;
        Ok(speakers
            .into_iter()
            .flat_map(|s| {
                let name = s.name;
                s.styles.into_iter().map(move |st| Voice {
                    id: st.id.to_string(),
                    label: format!("{name}（{}）", st.name),
                })
            })
            .collect())
    }

    async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>> {
        // 1. 読み・イントネーションのクエリを作る。応答は不透明な JSON のまま扱い、
        //    話速と音量だけを上書きする(エンジン間の差異やフィールド追加に強くするため)。
        let resp = self
            .client
            .post(self.url("audio_query"))
            .query(&[("speaker", voice_id), ("text", text)])
            .send()
            .await
            .map_err(unreachable)?;
        if !resp.status().is_success() {
            return Err(bad_response(format!("audio_query: HTTP {}", resp.status())));
        }
        let mut query: Value = resp.json().await.map_err(|e| bad_response(e))?;
        let obj = query.as_object_mut().ok_or_else(|| bad_response("audio_query の応答が JSON のオブジェクトではありません"))?;
        obj.insert("speedScale".into(), Value::from(p.speed_scale));
        obj.insert("volumeScale".into(), Value::from(p.volume));

        // 2. 合成。
        let resp = self
            .client
            .post(self.url("synthesis"))
            .query(&[("speaker", voice_id)])
            .json(&query)
            .send()
            .await
            .map_err(unreachable)?;
        if !resp.status().is_success() {
            return Err(bad_response(format!("synthesis: HTTP {}", resp.status())));
        }
        let bytes = resp.bytes().await.map_err(unreachable)?;
        if !bytes.starts_with(b"RIFF") {
            return Err(bad_response("synthesis の応答が WAV ではありません"));
        }
        Ok(bytes.to_vec())
    }
}
```

`#[allow(clippy::…)]` は不要。`crate::state::USER_AGENT` が `pub const` であることを確認する(`lib.rs` が `state::USER_AGENT` を使っているので存在する)。

Run: `cd src-tauri && cargo test --lib tts::external`
Expected: PASS(8件)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tts src-tauri/src/lib.rs
git commit -m "feat(tts): 音声合成エンジンの抽象とVOICEVOX互換クライアントを追加(Issue #625)"
```

---

### Task 4: 読み上げキュー(純粋な状態機械)

**Files:**
- Modify: `src-tauri/src/tts/queue.rs`(Task 3 で作った空ファイルを置き換える)

**Interfaces:**
- Consumes: `tts::engine::SynthParams`(Task 3)
- Produces:
  - `tts::queue::SpeechItem { note_id: String, column_id: String, text: String, voice_id: String, params: SynthParams }`(`Clone`、`Debug`)
  - `tts::queue::PushOutcome { Accepted { dropped: usize }, Duplicate }`
  - `tts::queue::SpeechQueue::new() -> Self`、`push(&mut self, item: SpeechItem, column_limit: usize) -> PushOutcome`、`pop(&mut self) -> Option<SpeechItem>`、`remove_column(&mut self, column_id: &str) -> usize`、`clear(&mut self)`、`len(&self) -> usize`

- [ ] **Step 1: テストを書く(失敗させる)**

`src-tauri/src/tts/queue.rs` を次の内容で置き換える(まずテストだけ)。

```rust
//! 読み上げの待ち行列(Issue #625)。音声・時刻・Tauri から独立した純粋なロジックで、単体テストで完結する。
//!
//! 方針: 全体で1本の FIFO(声が重ならないようにする)。上限は**カラムごと**で、あるカラムの待ち件数が
//! 上限を超えたら、そのカラムの最古の項目を捨てる。同じノート ID は二重に積まない。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::engine::SynthParams;

    fn item(note: &str, col: &str) -> SpeechItem {
        SpeechItem {
            note_id: note.into(),
            column_id: col.into(),
            text: format!("text-{note}"),
            voice_id: "1".into(),
            params: SynthParams { speed_scale: 1.0, volume: 1.0 },
        }
    }

    fn ids(q: &mut SpeechQueue) -> Vec<String> {
        std::iter::from_fn(|| q.pop()).map(|i| i.note_id).collect()
    }

    #[test]
    fn items_come_out_in_arrival_order_across_columns() {
        let mut q = SpeechQueue::new();
        q.push(item("n1", "a"), 5);
        q.push(item("n2", "b"), 5);
        q.push(item("n3", "a"), 5);
        assert_eq!(ids(&mut q), ["n1", "n2", "n3"]);
    }

    #[test]
    fn exceeding_a_columns_limit_drops_that_columns_oldest() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 2);
        q.push(item("b1", "b"), 2);
        q.push(item("a2", "a"), 2);
        let out = q.push(item("a3", "a"), 2);
        assert_eq!(out, PushOutcome::Accepted { dropped: 1 });
        assert_eq!(ids(&mut q), ["b1", "a2", "a3"], "a の最古(a1)だけが消え、b は無傷");
    }

    #[test]
    fn a_limit_of_one_keeps_only_the_newest_of_that_column() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 1);
        q.push(item("a2", "a"), 1);
        q.push(item("a3", "a"), 1);
        assert_eq!(ids(&mut q), ["a3"]);
    }

    #[test]
    fn a_limit_of_zero_is_treated_as_one() {
        let mut q = SpeechQueue::new();
        assert_eq!(q.push(item("a1", "a"), 0), PushOutcome::Accepted { dropped: 0 });
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn the_same_note_is_not_queued_twice_even_from_another_column() {
        let mut q = SpeechQueue::new();
        assert_eq!(q.push(item("n1", "a"), 5), PushOutcome::Accepted { dropped: 0 });
        assert_eq!(q.push(item("n1", "b"), 5), PushOutcome::Duplicate);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn a_note_already_taken_out_is_still_recognised_as_a_duplicate() {
        let mut q = SpeechQueue::new();
        q.push(item("n1", "a"), 5);
        assert!(q.pop().is_some());
        assert_eq!(q.push(item("n1", "a"), 5), PushOutcome::Duplicate, "読み上げ済みのノートは再度積まない");
    }

    #[test]
    fn the_recent_id_memory_is_bounded() {
        let mut q = SpeechQueue::new();
        for i in 0..(RECENT_IDS_MAX + 10) {
            q.push(item(&format!("n{i}"), "a"), 1000);
            q.pop();
        }
        // 十分古い ID は忘れる(メモリが増え続けない)。
        assert_eq!(q.push(item("n0", "a"), 5), PushOutcome::Accepted { dropped: 0 });
    }

    #[test]
    fn remove_column_removes_only_its_items_and_reports_the_count() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 5);
        q.push(item("b1", "b"), 5);
        q.push(item("a2", "a"), 5);
        assert_eq!(q.remove_column("a"), 2);
        assert_eq!(ids(&mut q), ["b1"]);
        assert_eq!(q.remove_column("nope"), 0);
    }

    #[test]
    fn clear_empties_the_queue() {
        let mut q = SpeechQueue::new();
        q.push(item("a1", "a"), 5);
        q.clear();
        assert_eq!(q.len(), 0);
        assert!(q.pop().is_none());
    }
}
```

Run: `cd src-tauri && cargo test --lib tts::queue`
Expected: FAIL(`SpeechQueue` が未定義)。

- [ ] **Step 2: 実装する**

`queue.rs` の先頭コメントの後(`#[cfg(test)]` の前)に追加する。

```rust
use super::engine::SynthParams;
use std::collections::VecDeque;

/// 重複排除のために覚えておく、直近のノート ID の数。
pub const RECENT_IDS_MAX: usize = 256;

#[derive(Debug, Clone)]
pub struct SpeechItem {
    pub note_id: String,
    pub column_id: String,
    /// フロントが整形した読み上げ文。
    pub text: String,
    /// 解決済みの声の ID(カラムの声、なければ既定の声)。
    pub voice_id: String,
    pub params: SynthParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// 積んだ。`dropped` は、上限を超えたために捨てた(そのカラムの最古の)件数。
    Accepted { dropped: usize },
    /// 同じノート ID が待ち中、または直近に読み上げ済みなので積まなかった。
    Duplicate,
}

#[derive(Debug, Default)]
pub struct SpeechQueue {
    items: VecDeque<SpeechItem>,
    /// 待ち中または直近に取り出したノート ID(重複排除用。古いものから忘れる)。
    recent: VecDeque<String>,
}

impl SpeechQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn push(&mut self, item: SpeechItem, column_limit: usize) -> PushOutcome {
        if self.recent.iter().any(|id| *id == item.note_id) {
            return PushOutcome::Duplicate;
        }
        let limit = column_limit.max(1);
        let column_id = item.column_id.clone();
        self.recent.push_back(item.note_id.clone());
        while self.recent.len() > RECENT_IDS_MAX {
            self.recent.pop_front();
        }
        self.items.push_back(item);

        let mut dropped = 0;
        while self.items.iter().filter(|i| i.column_id == column_id).count() > limit {
            // そのカラムの最古の項目を捨てる(必ず見つかる: 上のループ条件が真)。
            if let Some(pos) = self.items.iter().position(|i| i.column_id == column_id) {
                self.items.remove(pos);
                dropped += 1;
            }
        }
        PushOutcome::Accepted { dropped }
    }

    pub fn pop(&mut self) -> Option<SpeechItem> {
        self.items.pop_front()
    }

    /// 指定カラムの待ち項目を消し、消した件数を返す。
    pub fn remove_column(&mut self, column_id: &str) -> usize {
        let before = self.items.len();
        self.items.retain(|i| i.column_id != column_id);
        before - self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}
```

Run: `cd src-tauri && cargo test --lib tts::queue`
Expected: PASS(9件)。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tts/queue.rs
git commit -m "feat(tts): 上限付きの読み上げキューを追加(Issue #625)"
```

---

### Task 5: 読み上げレーンと `SoundPlayer` への統合

**Files:**
- Create: `src-tauri/src/speech_lane.rs`
- Modify: `src-tauri/src/sound.rs`(チャンネルを enum に、読み上げレーンの追加。`play()` の挙動と呼び出し側は変えない)
- Modify: `src-tauri/src/lib.rs`(`mod speech_lane;`)

**Interfaces:**
- Consumes: rodio 0.22 の `rodio::Player`(`connect_new` / `new` / `append` / `empty`)
- Produces:
  - `speech_lane::SpeakOutcome { Finished, Stopped, Failed(String) }`(`Debug`、`Clone`、`PartialEq`、`Eq`)、`speech_lane::DoneTx = tokio::sync::oneshot::Sender<SpeakOutcome>`
  - `speech_lane::SpeechLane::{new, is_active, start(player, buf, done), stop, fail(&str), poll() -> bool}`
  - `SoundPlayer`(`Clone` になる)に `pub async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome` と `pub fn stop_speech(&self)`

- [ ] **Step 1: レーンのテストを書く(失敗させる)**

`src-tauri/src/lib.rs` の `mod sound;` の次の行に `mod speech_lane;` を足す。

`src-tauri/src/speech_lane.rs`(まずテストだけ。`Player::new()` が返す `SourcesQueueOutput` を手で読み進めることで、デバイスなしで再生の進行をシミュレートする):

```rust
//! 読み上げの再生レーン(Issue #625)。`SoundPlayer` の再生スレッドの中で動く小さな部品で、
//! rodio の `Player`(`stop()` / `empty()` を持つ)を1発話ごとに作り、逐次再生・停止・完了通知を行う。
//! デバイスに触れないので、`Player::new()` が返すキューの出力を手で読み進めることで単体テストできる。

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use rodio::Player;
    use std::num::{NonZeroU16, NonZeroU32};
    use tokio::sync::oneshot;

    fn buf(len: usize) -> SamplesBuffer {
        SamplesBuffer::new(NonZeroU16::new(1).unwrap(), NonZeroU32::new(8000).unwrap(), vec![0.1f32; len])
    }

    /// キューの出力から `n` サンプルを読み進める(オーディオスレッドが再生するのと同じ動き)。
    fn drain(out: &mut impl Iterator<Item = f32>, n: usize) {
        for _ in 0..n {
            out.next();
        }
    }

    #[test]
    fn a_fresh_lane_is_idle() {
        let mut lane = SpeechLane::new();
        assert!(!lane.is_active());
        assert!(!lane.poll());
    }

    #[test]
    fn poll_reports_finished_once_the_buffer_has_been_played() {
        let mut lane = SpeechLane::new();
        let (player, mut out) = Player::new();
        let (tx, mut rx) = oneshot::channel();
        lane.start(player, buf(200), tx);
        assert!(lane.is_active());
        assert!(!lane.poll(), "まだ再生が終わっていない");
        assert!(rx.try_recv().is_err());

        drain(&mut out, 5_000);
        assert!(lane.poll(), "全サンプルを再生し終えた");
        assert_eq!(rx.try_recv().unwrap(), SpeakOutcome::Finished);
        assert!(!lane.is_active());
    }

    #[test]
    fn stop_ends_the_active_speech_with_stopped() {
        let mut lane = SpeechLane::new();
        let (player, _out) = Player::new();
        let (tx, mut rx) = oneshot::channel();
        lane.start(player, buf(200), tx);
        lane.stop();
        assert_eq!(rx.try_recv().unwrap(), SpeakOutcome::Stopped);
        assert!(!lane.is_active());
        lane.stop(); // 何もないときの stop は無害
    }

    #[test]
    fn starting_a_new_speech_stops_the_previous_one() {
        let mut lane = SpeechLane::new();
        let (p1, _o1) = Player::new();
        let (t1, mut r1) = oneshot::channel();
        lane.start(p1, buf(200), t1);
        let (p2, _o2) = Player::new();
        let (t2, mut r2) = oneshot::channel();
        lane.start(p2, buf(200), t2);
        assert_eq!(r1.try_recv().unwrap(), SpeakOutcome::Stopped);
        assert!(r2.try_recv().is_err());
        assert!(lane.is_active());
    }

    #[test]
    fn fail_reports_the_reason() {
        let mut lane = SpeechLane::new();
        let (player, _out) = Player::new();
        let (tx, mut rx) = oneshot::channel();
        lane.start(player, buf(200), tx);
        lane.fail("device reopened");
        assert_eq!(rx.try_recv().unwrap(), SpeakOutcome::Failed("device reopened".into()));
    }

    #[test]
    fn a_dropped_receiver_does_not_panic() {
        let mut lane = SpeechLane::new();
        let (player, _out) = Player::new();
        let (tx, rx) = oneshot::channel();
        drop(rx);
        lane.start(player, buf(200), tx);
        lane.stop(); // 受信側がいなくても panic しない
    }
}
```

Run: `cd src-tauri && cargo test --lib speech_lane`
Expected: FAIL(`SpeechLane` が未定義)。

- [ ] **Step 2: レーンを実装する**

`speech_lane.rs` の先頭コメントの後(`#[cfg(test)]` の前)に追加する。

```rust
use rodio::buffer::SamplesBuffer;
use rodio::Player;
use tokio::sync::oneshot;

/// 1発話の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeakOutcome {
    /// 最後まで再生した。
    Finished,
    /// 途中で止められた(`stop_speech`・新しい発話への置き換え)。
    Stopped,
    /// 再生できなかった(デコード失敗・出力デバイスの再オープン・再生スレッドの終了)。
    Failed(String),
}

pub type DoneTx = oneshot::Sender<SpeakOutcome>;

#[derive(Default)]
pub struct SpeechLane {
    /// 再生中の発話。`Player` を落とすと音が止まる(rodio の `Drop`)。
    active: Option<(Player, DoneTx)>,
}

impl SpeechLane {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// 発話を始める。再生中の発話があれば、先に `Stopped` で終わらせる。
    pub fn start(&mut self, player: Player, buf: SamplesBuffer, done: DoneTx) {
        self.finish(SpeakOutcome::Stopped);
        player.append(buf);
        self.active = Some((player, done));
    }

    pub fn stop(&mut self) {
        self.finish(SpeakOutcome::Stopped);
    }

    pub fn fail(&mut self, reason: &str) {
        self.finish(SpeakOutcome::Failed(reason.to_string()));
    }

    /// 再生が終わっていれば `Finished` を通知して `true`。再生中・何も無いときは `false`。
    pub fn poll(&mut self) -> bool {
        let finished = matches!(&self.active, Some((p, _)) if p.empty());
        if finished {
            self.finish(SpeakOutcome::Finished);
        }
        finished
    }

    fn finish(&mut self, outcome: SpeakOutcome) {
        if let Some((player, done)) = self.active.take() {
            drop(player);
            // 受信側(待っている側)が既にいなくても構わない。
            let _ = done.send(outcome);
        }
    }
}
```

Run: `cd src-tauri && cargo test --lib speech_lane`
Expected: PASS(6件)。もし `poll_reports_finished…` が失敗する場合は、`drain` のサンプル数を増やす前に、`rodio` の `Done` ラッパーがソース終端で `sound_count` を減らすタイミング(`SourcesQueueOutput::next` が次のソースへ進む時)を確認する。`drain(&mut out, 5_000)` は 200 サンプルのバッファに対して十分に余裕がある。

- [ ] **Step 3: `SoundPlayer` に統合する**

`src-tauri/src/sound.rs` を編集する。

(1) `use` を足す(既存の `use rodio::…` の近く)。

```rust
use crate::speech_lane::{DoneTx, SpeakOutcome, SpeechLane};
use rodio::Player;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
```

(2) `pub struct SoundPlayer { tx: mpsc::Sender<Vec<u8>>, }`(33〜35行目)を次に置き換える。

```rust
/// 再生スレッドへの命令。`Play` は従来どおりの投げっぱなしの通知音、`Speak` / `StopSpeech` は読み上げレーン(Issue #625)。
enum Cmd {
    Play(Vec<u8>),
    Speak { wav: Vec<u8>, done: DoneTx },
    StopSpeech,
}

/// 読み上げレーンの完了を確認する間隔。レーンが空いている間は、これまでどおりブロッキングで待つ(ポーリングしない)。
const LANE_POLL_INTERVAL: Duration = Duration::from_millis(40);

#[derive(Clone)]
pub struct SoundPlayer {
    tx: mpsc::Sender<Cmd>,
}
```

(3) `impl SoundPlayer` の `spawn()`、`play()`、`new_for_test()` を次に置き換える。`spawn` の「出力デバイスを開き直す」部分のログ文言は既存のままにする。

```rust
    /// 再生用バックグラウンドスレッドを起動する。出力デバイスが無い等で開けない
    /// 場合はログを残してスレッドを終了する(以降の`play`はログのみで何も鳴らさない)。
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::spawn(move || {
            let healthy = Arc::new(AtomicBool::new(true));
            let mut handle = match open_sink(&healthy) {
                Some(h) => h,
                None => return,
            };
            let mut lane = SpeechLane::new();

            loop {
                // 読み上げ中だけ短い間隔で起きて完了を確認する。何も再生していなければ、従来どおりブロッキングで待つ。
                let cmd = if lane.is_active() {
                    match rx.recv_timeout(LANE_POLL_INTERVAL) {
                        Ok(c) => Some(c),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                } else {
                    match rx.recv() {
                        Ok(c) => Some(c),
                        Err(_) => break,
                    }
                };
                lane.poll();
                let Some(cmd) = cmd else { continue };

                if !healthy.load(Ordering::SeqCst) {
                    // 出力デバイスを開き直すと、古い Mixer 上の読み上げは失われる。
                    lane.fail("出力デバイスを開き直したため読み上げを中断しました");
                    match open_sink(&healthy) {
                        Some(h) => {
                            handle = h;
                            log::warn!("通知音: 出力ストリーム異常を検知したため出力デバイスを再オープンしました");
                        }
                        None => {
                            log::warn!(
                                "通知音: 出力ストリーム異常を検知しましたが再オープンに失敗したため、既存のデバイスで再生を試みます"
                            );
                        }
                    }
                }

                match cmd {
                    Cmd::Play(bytes) => match decode_fully(bytes) {
                        DecodeOutcome::Ready(buf) => handle.mixer().add(buf),
                        DecodeOutcome::Invalid(e) => log::warn!("通知音: デコードに失敗: {e}"),
                        DecodeOutcome::Panicked => {
                            log::warn!("通知音: デコード処理がパニックしました(不正なファイルの可能性)。この音声はスキップします");
                        }
                    },
                    Cmd::Speak { wav, done } => match decode_fully(wav) {
                        DecodeOutcome::Ready(buf) => {
                            lane.start(Player::connect_new(handle.mixer()), buf, done);
                        }
                        DecodeOutcome::Invalid(e) => {
                            let _ = done.send(SpeakOutcome::Failed(format!("デコードに失敗: {e}")));
                        }
                        DecodeOutcome::Panicked => {
                            let _ = done.send(SpeakOutcome::Failed("デコード処理がパニックしました".into()));
                        }
                    },
                    Cmd::StopSpeech => lane.stop(),
                }
            }
        });
        Self { tx }
    }

    /// バイト列を非同期に再生する(デコード/再生はバックグラウンドスレッドで行うため即座に返る)。
    pub fn play(&self, bytes: Vec<u8>) {
        if self.tx.send(Cmd::Play(bytes)).is_err() {
            log::warn!("通知音: 再生スレッドが終了しているため再生できません");
        }
    }

    /// 読み上げの音声(WAV)を、読み上げレーンで再生し、終わる(または止められる・失敗する)まで待つ(Issue #625)。
    /// 通知音(`play`)とは Mixer 上でミックスされる。
    pub async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome {
        let (done, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(Cmd::Speak { wav, done }).is_err() {
            return SpeakOutcome::Failed("再生スレッドが終了しているため再生できません".into());
        }
        rx.await.unwrap_or_else(|_| SpeakOutcome::Failed("再生スレッドが終了しました".into()))
    }

    /// 再生中の読み上げを止める(待っている `speak` は `Stopped` で返る)。
    pub fn stop_speech(&self) {
        let _ = self.tx.send(Cmd::StopSpeech);
    }
```

`new_for_test()` は型だけ直す。

```rust
    pub(crate) fn new_for_test() -> Self {
        let (tx, _rx) = mpsc::channel::<Cmd>();
        Self { tx }
    }
```

(4) ファイル冒頭のドキュメントコメントの末尾(「ロギング基盤自体の変更は本 Issue のスコープ外)。」の後)に、次の段落を足す。

```rust
//!
//! 読み上げ(Issue #625)は、同じ再生スレッドの中に `SpeechLane`(`speech_lane.rs`)を持つ。
//! 通知音(`Cmd::Play`)とは別の `rodio::Player` で逐次再生し、完了・停止を `speak()` の戻り値で返す。
//! 通知音側の挙動(`play` の即時復帰・Mixer への重畳)は変えない。
```

- [ ] **Step 4: 既存のテストとビルドを確認する**

Run: `cd src-tauri && cargo test --lib sound:: && cargo test --lib speech_lane && cargo check --tests 2>&1 | tail -5`
Expected: PASS(`sound.rs` の既存のテスト、レーンのテスト)。コンパイルエラーなし。`SoundPlayer` が `Clone` になったことで `state.rs` 側の変更は不要。

- [ ] **Step 5: `speak` / `stop_speech` の経路のテストを足す(デバイスなし)**

`sound.rs` のテストモジュールの末尾に追加する(`new_for_test` はスレッドを立てず、`rx` を落とすので、送信は失敗して `Failed` で返る)。

```rust
    #[tokio::test]
    async fn speak_on_a_player_without_a_thread_fails_instead_of_hanging() {
        let p = SoundPlayer::new_for_test();
        let r = tokio::time::timeout(std::time::Duration::from_secs(2), p.speak(vec![0, 1, 2])).await;
        assert!(matches!(r, Ok(SpeakOutcome::Failed(_))), "{r:?}");
        p.stop_speech(); // 受信側がいなくても panic しない
    }
```

Run: `cd src-tauri && cargo test --lib sound::`
Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/speech_lane.rs src-tauri/src/sound.rs src-tauri/src/lib.rs
git commit -m "feat(tts): SoundPlayerに読み上げレーンを追加(Issue #625)"
```

---

### Task 6: ワーカー(`SpeechManager`)

**Files:**
- Modify: `src-tauri/src/tts/manager.rs`(空ファイルを置き換える)
- Modify: `src-tauri/src/state.rs`(`pub speech: std::sync::Arc<crate::tts::manager::SpeechManager>`)

**Interfaces:**
- Consumes: `tts::queue::{SpeechItem, SpeechQueue, PushOutcome}`(Task 4)、`tts::engine::{SpeechEngine, SynthParams}`(Task 3)、`speech_lane::SpeakOutcome`(Task 5)、`domain::SpeechSettings`(Task 2)、`sound::SoundPlayer::{speak, stop_speech}`(Task 5)
- Produces:
  - `tts::manager::SpeechStatus { Unknown, Ok, Error { message: String } }`(`Serialize` + `Type`、`tag = "type"`、`camelCase`、`Clone`、`PartialEq`、`Eq`)
  - `tts::manager::SpeechOutput`(`#[async_trait]`: `async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome`、`fn stop(&self)`)。`SoundPlayer` が実装する
  - `tts::manager::WorkerContext`(`settings() -> SpeechSettings`、`engine(&SpeechSettings) -> Result<Arc<dyn SpeechEngine>>`、`output() -> Arc<dyn SpeechOutput>`、`emit_status(SpeechStatus)`)
  - `tts::manager::SpeechManager::new() -> Self`、`enqueue(&self, item: SpeechItem, column_limit: usize) -> PushOutcome`、`remove_column(&self, column_id: &str) -> usize`、`stop_all(&self)`、`status(&self) -> SpeechStatus`
  - `tts::manager::run_loop(mgr: Arc<SpeechManager>, ctx: Arc<dyn WorkerContext>)`(無限ループ。呼び出し側が spawn する)、`tts::manager::handle_item(mgr: &SpeechManager, ctx: &dyn WorkerContext, item: SpeechItem)`(テスト用に `pub(crate)`)
  - `tts::manager::FAIL_THRESHOLD: u32 = 3`

- [ ] **Step 1: テストを書く(失敗させる)**

`src-tauri/src/tts/manager.rs` を次で置き換える(まずテストだけ)。

```rust
//! 読み上げのワーカー(Issue #625)。キューと状態を持つ `SpeechManager` と、
//! 「取り出す → 合成 → 再生 → 完了待ち → 次へ」を繰り返すループ。
//! エンジン・出力・設定・イベント送信は `WorkerContext` の裏に隠し、Tauri なしで単体テストできる。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SpeechSettings;
    use crate::error::Error;
    use crate::tts::engine::{SpeechEngine, SynthParams, Voice};
    use crate::tts::queue::SpeechItem;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    fn item(note: &str, col: &str) -> SpeechItem {
        SpeechItem {
            note_id: note.into(),
            column_id: col.into(),
            text: format!("text-{note}"),
            voice_id: "3".into(),
            params: SynthParams { speed_scale: 1.0, volume: 1.0 },
        }
    }

    /// 偽のエンジン。`fail` が真なら合成に失敗する。`on_synth` は合成の最中に呼ばれる(停止の競合の再現用)。
    struct FakeEngine {
        calls: Mutex<Vec<(String, String)>>,
        fail: Mutex<bool>,
        on_synth: Mutex<Option<Box<dyn Fn() + Send>>>,
    }
    impl FakeEngine {
        fn new() -> Arc<Self> {
            Arc::new(Self { calls: Default::default(), fail: Mutex::new(false), on_synth: Mutex::new(None) })
        }
    }
    #[async_trait]
    impl SpeechEngine for FakeEngine {
        async fn voices(&self) -> crate::error::Result<Vec<Voice>> {
            Ok(vec![])
        }
        async fn synthesize(&self, text: &str, voice_id: &str, _p: &SynthParams) -> crate::error::Result<Vec<u8>> {
            self.calls.lock().unwrap().push((text.into(), voice_id.into()));
            if let Some(f) = self.on_synth.lock().unwrap().as_ref() {
                f();
            }
            if *self.fail.lock().unwrap() {
                return Err(crate::tts::invalid(format!("{}: down", crate::tts::ERR_UNREACHABLE)));
            }
            Ok(format!("RIFF:{text}").into_bytes())
        }
    }

    #[derive(Default)]
    struct FakeOutput {
        played: Mutex<Vec<Vec<u8>>>,
        stops: AtomicUsize,
    }
    #[async_trait]
    impl SpeechOutput for FakeOutput {
        async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome {
            self.played.lock().unwrap().push(wav);
            SpeakOutcome::Finished
        }
        fn stop(&self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct FakeCtx {
        settings: Mutex<SpeechSettings>,
        engine: Arc<FakeEngine>,
        output: Arc<FakeOutput>,
        statuses: Mutex<Vec<SpeechStatus>>,
    }
    impl FakeCtx {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                settings: Mutex::new(SpeechSettings { enabled: true, external_url: "http://x".into(), ..Default::default() }),
                engine: FakeEngine::new(),
                output: Arc::new(FakeOutput::default()),
                statuses: Default::default(),
            })
        }
        fn played(&self) -> Vec<String> {
            self.output.played.lock().unwrap().iter().map(|b| String::from_utf8_lossy(b).into_owned()).collect()
        }
    }
    impl WorkerContext for FakeCtx {
        fn settings(&self) -> SpeechSettings {
            self.settings.lock().unwrap().clone()
        }
        fn engine(&self, _s: &SpeechSettings) -> crate::error::Result<Arc<dyn SpeechEngine>> {
            Ok(self.engine.clone())
        }
        fn output(&self) -> Arc<dyn SpeechOutput> {
            self.output.clone()
        }
        fn emit_status(&self, status: SpeechStatus) {
            self.statuses.lock().unwrap().push(status);
        }
    }

    #[tokio::test]
    async fn a_successful_item_is_synthesized_with_its_voice_played_and_marks_ok_once() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        handle_item(&mgr, ctx.as_ref(), item("n1", "a")).await;
        handle_item(&mgr, ctx.as_ref(), item("n2", "a")).await;

        assert_eq!(*ctx.engine.calls.lock().unwrap(), vec![("text-n1".to_string(), "3".to_string()), ("text-n2".to_string(), "3".to_string())]);
        assert_eq!(ctx.played(), ["RIFF:text-n1", "RIFF:text-n2"]);
        assert_eq!(*ctx.statuses.lock().unwrap(), vec![SpeechStatus::Ok], "状態が変わったときだけ1回通知する");
        assert_eq!(mgr.status(), SpeechStatus::Ok);
    }

    #[tokio::test]
    async fn an_item_is_spoken_even_when_the_master_switch_is_off_so_the_try_button_works() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        ctx.settings.lock().unwrap().enabled = false;
        handle_item(&mgr, ctx.as_ref(), item("n1", "__test__")).await;
        assert_eq!(ctx.played(), ["RIFF:text-n1"], "オフでも積まれた項目は読む(積む前の判定が責務)");
    }

    #[tokio::test]
    async fn three_consecutive_failures_report_an_error_once_and_clear_the_queue() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        *ctx.engine.fail.lock().unwrap() = true;
        for i in 0..5 {
            mgr.enqueue(item(&format!("q{i}"), "a"), 20);
        }
        for i in 0..FAIL_THRESHOLD {
            handle_item(&mgr, ctx.as_ref(), item(&format!("f{i}"), "a")).await;
        }
        let st = ctx.statuses.lock().unwrap().clone();
        assert_eq!(st.len(), 1, "閾値に達したときだけ1回: {st:?}");
        assert!(matches!(&st[0], SpeechStatus::Error { message } if message.contains(crate::tts::ERR_UNREACHABLE)), "{st:?}");
        assert!(mgr.pop_for_test().is_none(), "到達不能なら溜まった待ち項目は捨てる");

        // さらに失敗しても、同じ状態なので再通知しない。
        handle_item(&mgr, ctx.as_ref(), item("f9", "a")).await;
        assert_eq!(ctx.statuses.lock().unwrap().len(), 1);

        // 回復したら Ok に戻る。
        *ctx.engine.fail.lock().unwrap() = false;
        handle_item(&mgr, ctx.as_ref(), item("ok", "a")).await;
        assert_eq!(ctx.statuses.lock().unwrap().last(), Some(&SpeechStatus::Ok));
        assert_eq!(ctx.played(), ["RIFF:text-ok"]);
    }

    #[tokio::test]
    async fn a_failure_below_the_threshold_does_not_change_the_status() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        *ctx.engine.fail.lock().unwrap() = true;
        handle_item(&mgr, ctx.as_ref(), item("f1", "a")).await;
        handle_item(&mgr, ctx.as_ref(), item("f2", "a")).await;
        assert!(ctx.statuses.lock().unwrap().is_empty());
        // 途中で成功すれば、失敗の連続は数え直す。
        *ctx.engine.fail.lock().unwrap() = false;
        handle_item(&mgr, ctx.as_ref(), item("ok", "a")).await;
        *ctx.engine.fail.lock().unwrap() = true;
        handle_item(&mgr, ctx.as_ref(), item("f3", "a")).await;
        handle_item(&mgr, ctx.as_ref(), item("f4", "a")).await;
        assert_eq!(*ctx.statuses.lock().unwrap(), vec![SpeechStatus::Ok], "成功で 0 に戻っているので、あと1回失敗しても Error にならない");
    }

    #[tokio::test]
    async fn stopping_while_synthesizing_discards_the_audio() {
        let mgr = Arc::new(SpeechManager::new());
        let ctx = FakeCtx::new();
        let m2 = mgr.clone();
        *ctx.engine.on_synth.lock().unwrap() = Some(Box::new(move || m2.stop_all()));
        handle_item(&mgr, ctx.as_ref(), item("n1", "a")).await;
        assert!(ctx.played().is_empty(), "合成中に停止されたら、できあがった音声は再生しない");
        assert!(ctx.statuses.lock().unwrap().is_empty(), "停止は失敗ではない");
    }

    #[tokio::test]
    async fn stop_all_clears_the_queue() {
        let mgr = SpeechManager::new();
        mgr.enqueue(item("n1", "a"), 5);
        mgr.stop_all();
        assert!(mgr.pop_for_test().is_none());
    }

    #[tokio::test]
    async fn remove_column_drops_only_that_columns_pending_items() {
        let mgr = SpeechManager::new();
        mgr.enqueue(item("a1", "a"), 5);
        mgr.enqueue(item("b1", "b"), 5);
        assert_eq!(mgr.remove_column("a"), 1);
        assert_eq!(mgr.pop_for_test().unwrap().note_id, "b1");
    }

    #[tokio::test]
    async fn the_run_loop_speaks_queued_items_in_order() {
        let mgr = Arc::new(SpeechManager::new());
        let ctx = FakeCtx::new();
        let worker = tokio::spawn(run_loop(mgr.clone(), ctx.clone()));
        mgr.enqueue(item("n1", "a"), 5);
        mgr.enqueue(item("n2", "b"), 5);
        mgr.enqueue(item("n3", "a"), 5);

        tokio::time::timeout(Duration::from_secs(5), async {
            while ctx.played().len() < 3 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("3件とも再生される");
        assert_eq!(ctx.played(), ["RIFF:text-n1", "RIFF:text-n2", "RIFF:text-n3"]);
        worker.abort();
    }

    #[tokio::test]
    async fn an_engine_construction_error_counts_as_a_failure() {
        struct BadCtx(Arc<FakeCtx>);
        impl WorkerContext for BadCtx {
            fn settings(&self) -> SpeechSettings {
                self.0.settings()
            }
            fn engine(&self, _s: &SpeechSettings) -> crate::error::Result<Arc<dyn SpeechEngine>> {
                Err(Error::Invalid(crate::tts::ERR_NOT_CONFIGURED.into()))
            }
            fn output(&self) -> Arc<dyn SpeechOutput> {
                self.0.output()
            }
            fn emit_status(&self, s: SpeechStatus) {
                self.0.emit_status(s)
            }
        }
        let mgr = SpeechManager::new();
        let ctx = BadCtx(FakeCtx::new());
        for i in 0..FAIL_THRESHOLD {
            handle_item(&mgr, &ctx, item(&format!("n{i}"), "a")).await;
        }
        let st = ctx.0.statuses.lock().unwrap().clone();
        assert!(matches!(&st[..], [SpeechStatus::Error { message }] if message.contains(crate::tts::ERR_NOT_CONFIGURED)), "{st:?}");
    }
}
```

Run: `cd src-tauri && cargo test --lib tts::manager`
Expected: FAIL(`SpeechManager` などが未定義)。

- [ ] **Step 2: 実装する**

`manager.rs` の先頭コメントの後(`#[cfg(test)]` の前)に追加する。

```rust
use super::engine::SpeechEngine;
use super::queue::{PushOutcome, SpeechItem, SpeechQueue};
use crate::domain::SpeechSettings;
use crate::error::Result;
use crate::speech_lane::SpeakOutcome;
use async_trait::async_trait;
use serde::Serialize;
use specta::Type;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// 連続でこの回数、合成に失敗したら、エンジンに繋がらない状態(`Error`)とみなして待ち項目を捨てる。
pub const FAIL_THRESHOLD: u32 = 3;

/// 読み上げの状態(設定画面に見せる)。状態が変わったときだけ `SpeechStatusChanged` で通知する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SpeechStatus {
    /// まだ一度も合成していない。
    Unknown,
    /// 直近の合成が成功した。
    Ok,
    /// 連続して合成に失敗している。
    Error { message: String },
}

/// 合成した音声の出力先。本番は `SoundPlayer` の読み上げレーン。
#[async_trait]
pub trait SpeechOutput: Send + Sync {
    /// 再生して、終わる(または止められる・失敗する)まで待つ。
    async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome;
    /// 再生中の読み上げを止める。
    fn stop(&self);
}

#[async_trait]
impl SpeechOutput for crate::sound::SoundPlayer {
    async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome {
        crate::sound::SoundPlayer::speak(self, wav).await
    }
    fn stop(&self) {
        self.stop_speech();
    }
}

/// ワーカーが外の世界を見るための窓口。本番は `runtime::AppContext`(`AppHandle` 経由)、テストは偽の実装。
pub trait WorkerContext: Send + Sync {
    fn settings(&self) -> SpeechSettings;
    fn engine(&self, settings: &SpeechSettings) -> Result<Arc<dyn SpeechEngine>>;
    fn output(&self) -> Arc<dyn SpeechOutput>;
    fn emit_status(&self, status: SpeechStatus);
}

struct State {
    queue: SpeechQueue,
    status: SpeechStatus,
    /// 連続した合成の失敗回数。成功で 0 に戻る。
    failures: u32,
}

/// キューと状態。`AppState.speech` に常駐する(依存を持たないので `AppState::new` のシグネチャは変わらない)。
pub struct SpeechManager {
    state: Mutex<State>,
    wake: Notify,
    /// `stop_all` のたびに増える世代。合成中に停止されたかを、合成の前後の差で検出する。
    epoch: AtomicU64,
}

impl Default for SpeechManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeechManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State { queue: SpeechQueue::new(), status: SpeechStatus::Unknown, failures: 0 }),
            wake: Notify::new(),
            epoch: AtomicU64::new(0),
        }
    }

    /// 待ち行列に積み、ワーカーを起こす。
    pub fn enqueue(&self, item: SpeechItem, column_limit: usize) -> PushOutcome {
        let outcome = self.state.lock().unwrap().queue.push(item, column_limit);
        if matches!(outcome, PushOutcome::Accepted { .. }) {
            // 起きているワーカーがいなくても、許可証が残るので、次の `notified()` はすぐ返る。
            self.wake.notify_one();
        }
        outcome
    }

    /// 指定カラムの待ち項目を消す。再生中・合成中の1件は止めない。
    pub fn remove_column(&self, column_id: &str) -> usize {
        self.state.lock().unwrap().queue.remove_column(column_id)
    }

    /// 待ち項目をすべて消し、合成中の項目の音声を捨てさせる(世代を進める)。再生中の音声は、呼び出し側が
    /// `SpeechOutput::stop` で止める。
    pub fn stop_all(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.state.lock().unwrap().queue.clear();
    }

    pub fn status(&self) -> SpeechStatus {
        self.state.lock().unwrap().status.clone()
    }

    fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    fn pop(&self) -> Option<SpeechItem> {
        self.state.lock().unwrap().queue.pop()
    }

    #[cfg(test)]
    fn pop_for_test(&self) -> Option<SpeechItem> {
        self.pop()
    }

    /// 合成に成功した。状態が `Ok` に変わったら、通知すべき新しい状態を返す。
    fn record_success(&self) -> Option<SpeechStatus> {
        let mut s = self.state.lock().unwrap();
        s.failures = 0;
        if s.status == SpeechStatus::Ok {
            return None;
        }
        s.status = SpeechStatus::Ok;
        Some(SpeechStatus::Ok)
    }

    /// 合成に失敗した。連続失敗が閾値に達して `Error` に変わったら、新しい状態を返し、待ち項目を捨てる。
    fn record_failure(&self, message: String) -> Option<SpeechStatus> {
        let mut s = self.state.lock().unwrap();
        s.failures = s.failures.saturating_add(1);
        if s.failures < FAIL_THRESHOLD {
            return None;
        }
        s.queue.clear();
        let next = SpeechStatus::Error { message };
        if s.status == next {
            return None;
        }
        s.status = next.clone();
        Some(next)
    }
}

/// 1件を処理する。失敗してもパニックも早期リターンもせず、状態に反映するだけ(ノートの受信を妨げない)。
pub(crate) async fn handle_item(mgr: &SpeechManager, ctx: &dyn WorkerContext, item: SpeechItem) {
    let epoch = mgr.epoch();
    // 全体・カラムのオン・オフは、積む前(`enqueue_note_core`)と、オフにしたとき(`stop_all` /
    // `remove_column`)に処理済み。ここで `enabled` を見ないのは、設定画面の「試し聞き」を、
    // 全体スイッチがオフでも鳴らすため。
    let settings = ctx.settings();
    let engine = match ctx.engine(&settings) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("読み上げ: エンジンを用意できません: {e}");
            if let Some(st) = mgr.record_failure(e.to_string()) {
                ctx.emit_status(st);
            }
            return;
        }
    };
    let wav = match engine.synthesize(&item.text, &item.voice_id, &item.params).await {
        Ok(w) => w,
        Err(e) => {
            log::warn!("読み上げ: 合成に失敗しました: {e}");
            if let Some(st) = mgr.record_failure(e.to_string()) {
                ctx.emit_status(st);
            }
            return;
        }
    };
    // 合成が成功した = エンジンは生きている。
    if let Some(st) = mgr.record_success() {
        ctx.emit_status(st);
    }
    if mgr.epoch() != epoch {
        // 合成している間に停止された。できあがった音声は鳴らさない。
        return;
    }
    match ctx.output().speak(wav).await {
        SpeakOutcome::Finished | SpeakOutcome::Stopped => {}
        // 出力の失敗はエンジンの失敗ではない。状態には反映せず、ログに残して次へ進む。
        SpeakOutcome::Failed(m) => log::warn!("読み上げ: 再生に失敗しました: {m}"),
    }
}

/// ワーカーのループ。待ち行列から1件ずつ取り出して `handle_item` する。呼び出し側が spawn する。
pub async fn run_loop(mgr: Arc<SpeechManager>, ctx: Arc<dyn WorkerContext>) {
    loop {
        let item = loop {
            if let Some(i) = mgr.pop() {
                break i;
            }
            mgr.wake.notified().await;
        };
        handle_item(&mgr, ctx.as_ref(), item).await;
    }
}
```

`State` の `status` 比較に `PartialEq` を使うので、`SpeechStatus` は `PartialEq` を derive 済み。

Run: `cd src-tauri && cargo test --lib tts::manager`
Expected: PASS(9件)。

- [ ] **Step 3: `AppState` に載せる**

`src-tauri/src/state.rs` の `AppState` の `local_models` フィールドの後に追加する。

```rust
    /// 読み上げのキューと状態(Issue #625)。依存を持たないので、`AppState::new*` のシグネチャは変えずに
    /// 既定値で作る。ワーカーは起動時に `tts::runtime::spawn` が走らせる。
    pub speech: std::sync::Arc<crate::tts::manager::SpeechManager>,
```

`new_with_sound` の中の `local_models: std::sync::Arc::new(crate::translate::LocalModels::unconfigured()),`(268行目付近)の次の行に追加する。

```rust
            speech: std::sync::Arc::new(crate::tts::manager::SpeechManager::new()),
```

Run: `cd src-tauri && cargo check --tests 2>&1 | tail -5`
Expected: エラーなし。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tts/manager.rs src-tauri/src/state.rs
git commit -m "feat(tts): 読み上げのワーカーと状態管理を追加(Issue #625)"
```

---

### Task 7: コマンド・イベント・起動の配線

**Files:**
- Create: `src-tauri/src/commands/speech.rs`
- Modify: `src-tauri/src/tts/runtime.rs`(空ファイルを置き換える)
- Modify: `src-tauri/src/events.rs`(`SpeechStatusChanged`)
- Modify: `src-tauri/src/commands/mod.rs`(`pub mod speech;`)
- Modify: `src-tauri/src/commands/column.rs`(`close_column` で待ち項目を消す)
- Modify: `src-tauri/src/lib.rs`(コマンド・イベントの登録、`tts::runtime::spawn`)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: Task 2〜6 の成果物すべて
- Produces(TS に出るコマンド。引数は tauri-specta が camelCase の位置引数にする):
  - `getSpeechSettings(): SpeechSettings`、`setSpeechSettings(settings: SpeechSettings): SpeechSettings`
  - `setColumnSpeech(columnId: string, speech: ColumnSpeech): void`
  - `listSpeechVoices(externalUrl: string): Voice[]`
  - `speakNote(columnId: string, noteId: string, text: string): void`
  - `testSpeech(text: string): void`、`stopSpeech(): void`、`getSpeechStatus(): SpeechStatus`
  - Event `SpeechStatusChanged { status: SpeechStatus }`
  - Rust: `commands::speech::{get_settings_core, set_settings_core, set_column_speech_core, enqueue_note_core, test_speech_core}`(テスト用に `pub(crate)`)、`SPEECH_TEXT_MAX_CHARS: usize = 300`、`TEST_COLUMN_ID: &str = "__test__"`

- [ ] **Step 1: コマンドの中核のテストを書く(失敗させる)**

`src-tauri/src/commands/speech.rs` をまずテストだけで作る。

```rust
//! 読み上げのコマンド(Issue #625)。判断はすべて `*_core`(`AppState` だけに依存・Tauri なし)に置き、
//! `#[tauri::command]` はそれを呼ぶ薄い殻にする(`commands/mcp.rs` と同じ流儀)。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, ColumnKind, ColumnSpeech, FilterQuery, SpeechSettings};
    use crate::store::SettingsStore;
    use crate::tts::{ERR_NO_VOICE, ERR_NOT_CONFIGURED, ERR_TEXT_TOO_LONG};

    fn column(id: &str, speech: ColumnSpeech) -> Column {
        Column {
            id: id.into(),
            account_id: "a1".into(),
            kind: ColumnKind::Home,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
            speech,
        }
    }

    fn state_with(settings: SpeechSettings, cols: Vec<Column>) -> AppState {
        let store = SettingsStore::new_in_memory();
        store.update_speech(|s| *s = settings).unwrap();
        for c in &cols {
            store.upsert_column(c).unwrap();
        }
        AppState::new_for_test(store)
    }

    fn on() -> SpeechSettings {
        SpeechSettings { enabled: true, external_url: "http://h:1".into(), default_voice_id: "3".into(), ..Default::default() }
    }
    fn col_on() -> ColumnSpeech {
        ColumnSpeech { enabled: true, ..Default::default() }
    }
    fn pending(s: &AppState) -> Vec<crate::tts::queue::SpeechItem> {
        std::iter::from_fn(|| s.speech.pop_for_command_test()).collect()
    }

    #[test]
    fn set_settings_validates_normalizes_and_saves() {
        let s = state_with(SpeechSettings::default(), vec![]);
        let bad = SpeechSettings { external_url: "ftp://x".into(), ..Default::default() };
        assert!(set_settings_core(&s, bad).is_err());

        let got = set_settings_core(&s, SpeechSettings { external_url: "  http://h:1/  ".into(), ..on() }).unwrap();
        assert_eq!(got.external_url, "http://h:1", "前後の空白と末尾のスラッシュを落として保存する");
        assert_eq!(s.settings.load_speech().external_url, "http://h:1");
    }

    #[test]
    fn turning_the_master_switch_off_clears_the_queue() {
        let s = state_with(on(), vec![column("c1", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "こんにちは").unwrap();
        set_settings_core(&s, SpeechSettings { enabled: false, ..on() }).unwrap();
        assert!(pending(&s).is_empty());
    }

    #[test]
    fn set_column_speech_normalizes_and_clears_that_columns_queue_when_turned_off() {
        let s = state_with(on(), vec![column("c1", col_on()), column("c2", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        enqueue_note_core(&s, "c2", "n2", "b").unwrap();

        set_column_speech_core(&s, "c1", ColumnSpeech { enabled: false, queue_limit: 99, ..Default::default() }).unwrap();

        assert_eq!(s.settings.load_column("c1").unwrap().speech.queue_limit, crate::domain::speech::QUEUE_LIMIT_MAX);
        let left = pending(&s);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].note_id, "n2", "オフにしたカラムの待ち項目だけが消える");
    }

    #[test]
    fn set_column_speech_on_a_missing_column_is_not_found() {
        let s = state_with(on(), vec![]);
        assert!(matches!(set_column_speech_core(&s, "nope", ColumnSpeech::default()), Err(crate::error::Error::NotFound(_))));
    }

    #[test]
    fn enqueue_resolves_the_column_voice_then_falls_back_to_the_default_voice() {
        let s = state_with(on(), vec![column("c1", col_on()), column("c2", ColumnSpeech { voice_id: "9".into(), ..col_on() })]);
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        enqueue_note_core(&s, "c2", "n2", "b").unwrap();
        let q = pending(&s);
        assert_eq!(q[0].voice_id, "3", "カラムの声が空なら既定の声");
        assert_eq!(q[1].voice_id, "9", "カラムの声が優先");
        assert_eq!(q[0].params.speed_scale, 1.0);
    }

    #[test]
    fn enqueue_is_a_silent_noop_when_speech_is_off_globally_or_for_the_column() {
        let s = state_with(SpeechSettings { enabled: false, ..on() }, vec![column("c1", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        let s2 = state_with(on(), vec![column("c1", ColumnSpeech::default())]);
        enqueue_note_core(&s2, "c1", "n1", "a").unwrap();
        assert!(pending(&s).is_empty());
        assert!(pending(&s2).is_empty());
    }

    #[test]
    fn enqueue_ignores_blank_text_and_rejects_overlong_text() {
        let s = state_with(on(), vec![column("c1", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "  \n  ").unwrap();
        assert!(pending(&s).is_empty());

        let long = "あ".repeat(SPEECH_TEXT_MAX_CHARS + 1);
        let err = enqueue_note_core(&s, "c1", "n2", &long).unwrap_err();
        assert!(err.to_string().contains(ERR_TEXT_TOO_LONG), "{err}");
        enqueue_note_core(&s, "c1", "n3", &"あ".repeat(SPEECH_TEXT_MAX_CHARS)).unwrap();
        assert_eq!(pending(&s).len(), 1, "上限ちょうどは通る");
    }

    #[test]
    fn enqueue_reports_a_missing_column_url_or_voice() {
        let s = state_with(on(), vec![]);
        assert!(matches!(enqueue_note_core(&s, "nope", "n", "a"), Err(crate::error::Error::NotFound(_))));

        let s = state_with(SpeechSettings { external_url: String::new(), ..on() }, vec![column("c1", col_on())]);
        let e = enqueue_note_core(&s, "c1", "n", "a").unwrap_err();
        assert!(e.to_string().contains(ERR_NOT_CONFIGURED), "{e}");

        let s = state_with(SpeechSettings { default_voice_id: String::new(), ..on() }, vec![column("c1", col_on())]);
        let e = enqueue_note_core(&s, "c1", "n", "a").unwrap_err();
        assert!(e.to_string().contains(ERR_NO_VOICE), "{e}");
    }

    #[test]
    fn the_same_note_from_two_columns_is_queued_once() {
        let s = state_with(on(), vec![column("c1", col_on()), column("c2", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        enqueue_note_core(&s, "c2", "n1", "a").unwrap();
        assert_eq!(pending(&s).len(), 1);
    }

    #[test]
    fn the_column_limit_is_applied_per_column() {
        let s = state_with(on(), vec![column("c1", ColumnSpeech { queue_limit: 2, ..col_on() })]);
        for i in 0..4 {
            enqueue_note_core(&s, "c1", &format!("n{i}"), "a").unwrap();
        }
        let ids: Vec<_> = pending(&s).into_iter().map(|i| i.note_id).collect();
        assert_eq!(ids, ["n2", "n3"], "古い方から捨てる");
    }

    #[test]
    fn test_speech_queues_under_the_test_column_even_when_the_master_switch_is_off() {
        let s = state_with(SpeechSettings { enabled: false, ..on() }, vec![]);
        test_speech_core(&s, "テストです").unwrap();
        let q = pending(&s);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].column_id, TEST_COLUMN_ID);
    }
}
```

注: `pending()` はキューの中身を取り出すテスト用の窓口が要る。`SpeechManager` に `#[cfg(test)] pub(crate) fn pop_for_command_test(&self) -> Option<SpeechItem>` を足す(Step 2 で実装する)。

Run: `cd src-tauri && cargo test --lib commands::speech`
Expected: FAIL(`set_settings_core` などが未定義)。

- [ ] **Step 2: コアとコマンドを実装する**

`src-tauri/src/tts/manager.rs` の `pop_for_test` の直後に、他モジュールのテストから使う窓口を足す。

```rust
    /// 他モジュールのテストから、待ち項目を1件取り出す(`pop_for_test` は同モジュール専用)。
    #[cfg(test)]
    pub(crate) fn pop_for_command_test(&self) -> Option<SpeechItem> {
        self.pop()
    }
```

`src-tauri/src/commands/speech.rs` の先頭コメントの後(`#[cfg(test)]` の前)に追加する。

```rust
use crate::domain::speech::{QUEUE_LIMIT_DEFAULT, QUEUE_LIMIT_MAX};
use crate::domain::{ColumnSpeech, SpeechSettings};
use crate::error::{Error, Result};
use crate::state::AppState;
use crate::tts::engine::{SpeechEngine, SynthParams, Voice};
use crate::tts::external::ExternalEngine;
use crate::tts::manager::SpeechStatus;
use crate::tts::queue::SpeechItem;
use crate::tts::{invalid, ERR_NOT_CONFIGURED, ERR_NO_VOICE, ERR_TEXT_TOO_LONG};
use tauri::State;

/// 1件の読み上げ文の最大長(文字数)。フロントは 120 文字(+投稿者名)に切るので、これは不正な入力への備え。
pub const SPEECH_TEXT_MAX_CHARS: usize = 300;
/// 「試し聞き」を積むときの擬似カラム ID。実在のカラムの待ち項目とは混ざらない。
pub const TEST_COLUMN_ID: &str = "__test__";

pub(crate) fn get_settings_core(s: &AppState) -> SpeechSettings {
    s.settings.load_speech()
}

/// 検証・整形して保存する。オフにしたら、待ち行列を空にして再生中の読み上げも止める。
pub(crate) fn set_settings_core(s: &AppState, mut settings: SpeechSettings) -> Result<SpeechSettings> {
    settings.validate().map_err(Error::Invalid)?;
    let url = settings.external_url.trim().trim_end_matches('/').to_string();
    settings.external_url = url;
    settings.default_voice_id = settings.default_voice_id.trim().to_string();
    let saved = s.settings.update_speech(|v| *v = settings)?;
    if !saved.enabled {
        s.speech.stop_all();
        s.sound.stop_speech();
    }
    Ok(saved)
}

/// カラムの読み上げ設定を保存する。オフにしたら、そのカラムの待ち項目だけを消す。
pub(crate) fn set_column_speech_core(s: &AppState, column_id: &str, speech: ColumnSpeech) -> Result<()> {
    let speech = speech.normalized();
    let enabled = speech.enabled;
    if !s.settings.set_column_speech(column_id, speech)? {
        return Err(Error::NotFound(format!("column {column_id}")));
    }
    if !enabled {
        s.speech.remove_column(column_id);
    }
    Ok(())
}

fn params_of(settings: &SpeechSettings) -> SynthParams {
    SynthParams { speed_scale: settings.speed_scale, volume: settings.volume }
}

/// 整形済みの読み上げ文を、キューに積む。全体・カラムのどちらかがオフなら、何もせず `Ok`
/// (設定を切り替えた直後の競合を許す)。
pub(crate) fn enqueue_note_core(s: &AppState, column_id: &str, note_id: &str, text: &str) -> Result<()> {
    let settings = s.settings.load_speech();
    let column = s
        .settings
        .load_column(column_id)
        .ok_or_else(|| Error::NotFound(format!("column {column_id}")))?;
    if !settings.enabled || !column.speech.enabled {
        return Ok(());
    }
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    if text.chars().count() > SPEECH_TEXT_MAX_CHARS {
        return Err(invalid(ERR_TEXT_TOO_LONG));
    }
    if settings.external_url.trim().is_empty() {
        return Err(invalid(ERR_NOT_CONFIGURED));
    }
    let voice_id = if column.speech.voice_id.is_empty() { settings.default_voice_id.clone() } else { column.speech.voice_id.clone() };
    if voice_id.is_empty() {
        return Err(invalid(ERR_NO_VOICE));
    }
    s.speech.enqueue(
        SpeechItem {
            note_id: note_id.to_string(),
            column_id: column_id.to_string(),
            text: text.to_string(),
            voice_id,
            params: params_of(&settings),
        },
        column.speech.queue_limit.clamp(1, QUEUE_LIMIT_MAX) as usize,
    );
    Ok(())
}

/// 設定画面の「試し聞き」。全体のスイッチがオフでも鳴らす(設定を確かめるためのボタンなので)。
pub(crate) fn test_speech_core(s: &AppState, text: &str) -> Result<()> {
    let settings = s.settings.load_speech();
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    if text.chars().count() > SPEECH_TEXT_MAX_CHARS {
        return Err(invalid(ERR_TEXT_TOO_LONG));
    }
    if settings.external_url.trim().is_empty() {
        return Err(invalid(ERR_NOT_CONFIGURED));
    }
    if settings.default_voice_id.is_empty() {
        return Err(invalid(ERR_NO_VOICE));
    }
    s.speech.enqueue(
        SpeechItem {
            // 毎回ちがう ID にして、キューの重複排除に弾かれないようにする。
            note_id: format!("test-{}", uuid::Uuid::new_v4()),
            column_id: TEST_COLUMN_ID.to_string(),
            text: text.to_string(),
            voice_id: settings.default_voice_id.clone(),
            params: params_of(&settings),
        },
        QUEUE_LIMIT_DEFAULT as usize,
    );
    Ok(())
}

// ---- Tauri コマンド(薄い殻) ----

#[tauri::command]
#[specta::specta]
pub async fn get_speech_settings(state: State<'_, AppState>) -> Result<SpeechSettings> {
    Ok(get_settings_core(&state))
}

#[tauri::command]
#[specta::specta]
pub async fn set_speech_settings(state: State<'_, AppState>, settings: SpeechSettings) -> Result<SpeechSettings> {
    set_settings_core(&state, settings)
}

#[tauri::command]
#[specta::specta]
pub async fn set_column_speech(state: State<'_, AppState>, column_id: String, speech: ColumnSpeech) -> Result<()> {
    set_column_speech_core(&state, &column_id, speech)
}

/// 入力欄の URL(未保存でよい)のエンジンから、選べる声を取得する。設定画面の「接続テスト」。
#[tauri::command]
#[specta::specta]
pub async fn list_speech_voices(external_url: String) -> Result<Vec<Voice>> {
    ExternalEngine::new(&external_url)?.voices().await
}

/// フロントが整形した読み上げ文を依頼する。失敗しても、フロントは握りつぶす(通知音と同じ fire-and-forget)。
#[tauri::command]
#[specta::specta]
pub async fn speak_note(state: State<'_, AppState>, column_id: String, note_id: String, text: String) -> Result<()> {
    enqueue_note_core(&state, &column_id, &note_id, &text)
}

#[tauri::command]
#[specta::specta]
pub async fn test_speech(state: State<'_, AppState>, text: String) -> Result<()> {
    test_speech_core(&state, &text)
}

/// 待ち行列を空にして、再生中の読み上げを止める。
#[tauri::command]
#[specta::specta]
pub async fn stop_speech(state: State<'_, AppState>) -> Result<()> {
    state.speech.stop_all();
    state.sound.stop_speech();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_speech_status(state: State<'_, AppState>) -> Result<SpeechStatus> {
    Ok(state.speech.status())
}
```

Run: `cd src-tauri && cargo test --lib commands::speech`
Expected: PASS(11件)。

- [ ] **Step 3: 本番の `WorkerContext` と起動(`runtime.rs`)**

`src-tauri/src/tts/runtime.rs` を次で置き換える。

```rust
//! 読み上げワーカーの本番の配線(Issue #625)。`AppHandle` 経由で設定・出力・イベントを引く。

use super::engine::SpeechEngine;
use super::external::ExternalEngine;
use super::manager::{run_loop, SpeechOutput, SpeechStatus, WorkerContext};
use super::{invalid, ERR_NOT_CONFIGURED};
use crate::domain::SpeechSettings;
use crate::error::Result;
use crate::events::SpeechStatusChanged;
use crate::state::AppState;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;

pub struct AppContext {
    app: AppHandle,
    /// 直近に作ったエンジンを、URL が同じ間は使い回す(接続の再利用)。
    engine: Mutex<Option<(String, Arc<dyn SpeechEngine>)>>,
}

impl AppContext {
    pub fn new(app: AppHandle) -> Self {
        Self { app, engine: Mutex::new(None) }
    }
}

impl WorkerContext for AppContext {
    fn settings(&self) -> SpeechSettings {
        self.app.state::<AppState>().settings.load_speech()
    }

    fn engine(&self, settings: &SpeechSettings) -> Result<Arc<dyn SpeechEngine>> {
        let url = settings.external_url.trim().to_string();
        if url.is_empty() {
            return Err(invalid(ERR_NOT_CONFIGURED));
        }
        let mut cached = self.engine.lock().unwrap();
        if let Some((u, e)) = cached.as_ref() {
            if *u == url {
                return Ok(e.clone());
            }
        }
        let e: Arc<dyn SpeechEngine> = Arc::new(ExternalEngine::new(&url)?);
        *cached = Some((url, e.clone()));
        Ok(e)
    }

    fn output(&self) -> Arc<dyn SpeechOutput> {
        Arc::new(self.app.state::<AppState>().sound.clone())
    }

    fn emit_status(&self, status: SpeechStatus) {
        let _ = SpeechStatusChanged { status }.emit(&self.app);
    }
}

/// ワーカーを起動する。`AppState` を `manage` した後に呼ぶ。
pub fn spawn(app: AppHandle) {
    let mgr = app.state::<AppState>().speech.clone();
    let ctx: Arc<dyn WorkerContext> = Arc::new(AppContext::new(app));
    tauri::async_runtime::spawn(run_loop(mgr, ctx));
}
```

- [ ] **Step 4: イベント・登録・起動・`close_column` 連動**

`src-tauri/src/events.rs` の `LocalModelStatusChanged` の後に追加する。

```rust

/// 読み上げの状態が変わった(Issue #625)。状態が変わったときだけ送る(連続失敗でエラー、回復で Ok)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatusChanged {
    pub status: crate::tts::manager::SpeechStatus,
}
```

`SpeechStatus` は `Serialize` と `Type` のみで `Deserialize` を持たないので、**`Deserialize` を足す**。`tts/manager.rs` の `SpeechStatus` の derive を次にする。

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
```

そして `manager.rs` の `use serde::Serialize;` を `use serde::{Deserialize, Serialize};` にする。

`src-tauri/src/commands/mod.rs` の `pub mod sound;` の次に `pub mod speech;` を足す。

`src-tauri/src/lib.rs` を編集する。`specta_builder()` の `commands::mcp::regenerate_mcp_token,` の後に追加する。

```rust
            commands::speech::get_speech_settings,
            commands::speech::set_speech_settings,
            commands::speech::set_column_speech,
            commands::speech::list_speech_voices,
            commands::speech::speak_note,
            commands::speech::test_speech,
            commands::speech::stop_speech,
            commands::speech::get_speech_status,
```

`collect_events![` の `events::LocalModelStatusChanged,` の後に `events::SpeechStatusChanged,` を足す。

`setup` の `scheduler::spawn(app.handle().clone());` の次の行に追加する。

```rust
            // 読み上げのワーカー(Issue #625)。キューが空の間は待つだけで、設定がオフなら何もしない。
            tts::runtime::spawn(app.handle().clone());
```

`src-tauri/src/commands/column.rs` の `close_column`(`grep -n "pub async fn close_column" src-tauri/src/commands/column.rs` で場所を探す)の中で、カラムを設定から削除した直後に、読み上げの待ち項目を消す1行を足す。

```rust
    // 閉じたカラムの読み上げの待ち項目は、もう読まない(Issue #625)。
    state.speech.remove_column(&column_id);
```

(`close_column` の引数名とステートの変数名は、その関数の既存の呼び出しに合わせる。`state.settings.delete_column(&column_id)` のような行の直後に置く。)

- [ ] **Step 5: ビルドとバインディングの再生成**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -15`
Expected: PASS。

Run: `cd src-tauri && cargo test generates_frontend_bindings 2>&1 | tail -5`
Expected: PASS。続いて、新しいコマンド・型が TS に出ていることを確認する。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && grep -nE "getSpeechSettings|setColumnSpeech|speakNote|listSpeechVoices|export type (ColumnSpeech|SpeechSettings|SpeechStatus|Voice|SpeechEngineKind)|speechStatusChanged" frontend/src/bindings/tauri.gen.ts | head -20
git diff --stat frontend/src/bindings/tauri.gen.ts
```

Expected: 上記の名前がすべてヒットし、`Column` に `speech: ColumnSpeech,` が増えている。`tokens` が型に出ていないことは既存のテスト(`generates_frontend_bindings`)が検証する。

- [ ] **Step 6: `close_column` の連動をテストする**

`src-tauri/src/commands/speech.rs` のテストに追加する(`close_column` の実体を呼ぶのが重い場合は、`remove_column` の連動を、設定からカラムを消す既存の純粋な経路で確認する形でよい。`SettingsStore::delete_column` → `speech.remove_column` の2行を、ここでは回帰テストとして固定する)。

```rust
    #[test]
    fn closing_a_column_drops_its_pending_items() {
        let s = state_with(on(), vec![column("c1", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        // close_column が行う処理(設定から削除 + 待ち項目の削除)。
        s.settings.delete_column("c1").unwrap();
        s.speech.remove_column("c1");
        assert!(pending(&s).is_empty());
    }
```

Run: `cd src-tauri && cargo test --lib commands::speech`
Expected: PASS(12件)。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src frontend/src/bindings/tauri.gen.ts
git commit -m "feat(tts): 読み上げのコマンドとイベントを追加(Issue #625)"
```

---

### Task 8: フロントの読み上げ文の整形(`speechText.ts`)

**Files:**
- Create: `frontend/src/lib/speechText.ts`、`frontend/src/lib/speechText.test.ts`

**Interfaces:**
- Consumes: `mfm-js` の `parse` / `parseSimple` / `MfmNode`、バインディングの `Note` / `ColumnSpeech`(Task 7 で再生成済み)
- Produces:
  - `MAX_SPEECH_CHARS = 120`、`DEFAULT_COLUMN_SPEECH: ColumnSpeech`
  - `buildSpeechText(note: Note, opts: { speakAuthor: boolean }): string | null`
  - `speechTextForColumnNote(note: Note, ctx: { isOwn: boolean; globalEnabled: boolean; column: ColumnSpeech }): string | null`

- [ ] **Step 1: テストを書く(失敗させる)**

`frontend/src/lib/speechText.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { Note, User } from "../bindings/tauri.gen";
import {
  buildSpeechText,
  DEFAULT_COLUMN_SPEECH,
  MAX_SPEECH_CHARS,
  speechTextForColumnNote,
} from "./speechText";

function user(over: Partial<User> = {}): User {
  return {
    id: "u1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: null,
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    ...over,
  } as User;
}

function note(over: Partial<Note> = {}): Note {
  return {
    id: "n1",
    createdAt: 0,
    text: "こんにちは",
    cw: null,
    visibility: "public",
    localOnly: false,
    user: user(),
    replyId: null,
    replyUserId: null,
    reply: null,
    renoteId: null,
    renote: null,
    files: [],
    poll: null,
    tags: [],
    mentions: [],
    emojis: {},
    channelId: null,
    via: null,
    ...over,
  } as Note;
}

const noAuthor = { speakAuthor: false };
const withAuthor = { speakAuthor: true };

describe("buildSpeechText: 何を読むか", () => {
  it("本文をそのまま読む", () => {
    expect(buildSpeechText(note({ text: "今日はいい天気" }), noAuthor)).toBe("今日はいい天気");
  });

  it("CW があれば CW だけを読み、本文は読まない", () => {
    const n = note({ cw: "ネタバレ注意", text: "犯人は〇〇" });
    expect(buildSpeechText(n, noAuthor)).toBe("ネタバレ注意");
  });

  it("純粋なリノート(本文なし)は読まない", () => {
    expect(buildSpeechText(note({ text: null, renoteId: "r1", renote: note({ id: "r1" }) }), noAuthor)).toBeNull();
  });

  it("引用リノートは、コメント部分だけを読む", () => {
    const n = note({ text: "これ好き", renoteId: "r1", renote: note({ id: "r1", text: "元のノート" }) });
    expect(buildSpeechText(n, noAuthor)).toBe("これ好き");
  });

  it("画像だけのノート(本文なし)は読まない", () => {
    expect(buildSpeechText(note({ text: null, files: [{ id: "f" } as never] }), noAuthor)).toBeNull();
  });

  it("空白・改行だけの本文は読まない", () => {
    expect(buildSpeechText(note({ text: "  \n 　 " }), noAuthor)).toBeNull();
    expect(buildSpeechText(note({ text: "" }), noAuthor)).toBeNull();
  });
});

describe("buildSpeechText: MFM の整形", () => {
  it("装飾(太字・small・fn)は中身をそのまま読む", () => {
    expect(buildSpeechText(note({ text: "**強調**と<small>小さい</small>と$[tada ほげ]" }), noAuthor)).toBe(
      "強調と小さいとほげ",
    );
  });

  it("メンションはユーザー名だけ(ホストは読まない)、ハッシュタグは # を除く", () => {
    expect(buildSpeechText(note({ text: "@bob@example.com こんにちは #rust" }), noAuthor)).toBe("bob こんにちは rust");
  });

  it("URL は「URL」と読み、ラベル付きリンクはラベルを読む", () => {
    expect(buildSpeechText(note({ text: "見て https://example.com/a?b=c です" }), noAuthor)).toBe("見て URL です");
    expect(buildSpeechText(note({ text: "[公式サイト](https://example.com)です" }), noAuthor)).toBe("公式サイトです");
  });

  it("コードは内容を読まず「コード」と読む", () => {
    expect(buildSpeechText(note({ text: "実行は`rm -rf /`で" }), noAuthor)).toBe("実行はコードで");
    expect(buildSpeechText(note({ text: "```\nfn main() {}\n```" }), noAuthor)).toBe("コード");
  });

  it("カスタム絵文字・Unicode 絵文字・数式は読まない", () => {
    expect(buildSpeechText(note({ text: "やった:tada:🎉\\(x^2\\)" }), noAuthor)).toBe("やった");
  });

  it("絵文字・URL だけでも、URL は読む。絵文字だけなら読まない", () => {
    expect(buildSpeechText(note({ text: "🎉🎉:blob:" }), noAuthor)).toBeNull();
    expect(buildSpeechText(note({ text: "https://example.com" }), noAuthor)).toBe("URL");
  });

  it("改行は読点にし、句読点の直後の改行は足さない。連続する改行も1つにする", () => {
    expect(buildSpeechText(note({ text: "一行目\n二行目" }), noAuthor)).toBe("一行目、二行目");
    expect(buildSpeechText(note({ text: "一行目。\n二行目" }), noAuthor)).toBe("一行目。二行目");
    expect(buildSpeechText(note({ text: "a\n\n\nb" }), noAuthor)).toBe("a、b");
  });

  it("先頭・末尾の読点や空白は落とす", () => {
    expect(buildSpeechText(note({ text: "\n\nこんにちは\n" }), noAuthor)).toBe("こんにちは");
  });
});

describe("buildSpeechText: 長さ", () => {
  it("上限ちょうどは切らない", () => {
    const text = "あ".repeat(MAX_SPEECH_CHARS);
    expect(buildSpeechText(note({ text }), noAuthor)).toBe(text);
  });

  it("上限を超えたら、切って「以下略」を付ける", () => {
    const out = buildSpeechText(note({ text: "あ".repeat(MAX_SPEECH_CHARS + 50) }), noAuthor)!;
    expect(out).toBe("あ".repeat(MAX_SPEECH_CHARS) + "、以下略");
  });

  it("サロゲートペアを途中で割らない", () => {
    const out = buildSpeechText(note({ text: "𠮷".repeat(MAX_SPEECH_CHARS + 5) }), noAuthor)!;
    expect(out).toBe("𠮷".repeat(MAX_SPEECH_CHARS) + "、以下略");
  });

  it("巨大な本文でも一瞬で終わる(数万文字)", () => {
    const started = performance.now();
    const out = buildSpeechText(note({ text: "あいう ".repeat(20_000) }), noAuthor)!;
    expect(performance.now() - started).toBeLessThan(1000);
    expect(out.endsWith("、以下略")).toBe(true);
  });

  it("深くネストした MFM でも落ちない", () => {
    const text = "**".repeat(0) + "<small>".repeat(200) + "中身" + "</small>".repeat(200);
    expect(() => buildSpeechText(note({ text }), noAuthor)).not.toThrow();
  });
});

describe("buildSpeechText: 投稿者名", () => {
  it("speakAuthor がオンなら「{表示名}さん、」を前置きする", () => {
    expect(buildSpeechText(note({ text: "おはよう" }), withAuthor)).toBe("Aliceさん、おはよう");
  });

  it("表示名のカスタム絵文字は読まない。空になったら username を読む", () => {
    expect(buildSpeechText(note({ text: "おはよう", user: user({ name: "Alice:blob:" }) }), withAuthor)).toBe("Aliceさん、おはよう");
    expect(buildSpeechText(note({ text: "おはよう", user: user({ name: ":blob:" }) }), withAuthor)).toBe("aliceさん、おはよう");
    expect(buildSpeechText(note({ text: "おはよう", user: user({ name: null }) }), withAuthor)).toBe("aliceさん、おはよう");
  });

  it("読む中身がなければ、投稿者名だけを読み上げない", () => {
    expect(buildSpeechText(note({ text: "🎉" }), withAuthor)).toBeNull();
    expect(buildSpeechText(note({ text: null }), withAuthor)).toBeNull();
  });

  it("CW のノートでも投稿者名を前置きする", () => {
    expect(buildSpeechText(note({ cw: "注意", text: "本文" }), withAuthor)).toBe("Aliceさん、注意");
  });
});

describe("speechTextForColumnNote: 発火の判定", () => {
  const col = { ...DEFAULT_COLUMN_SPEECH, enabled: true };

  it("全部の条件が揃ったときだけ文字列を返す", () => {
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: true, column: col })).toBe("Aliceさん、こんにちは");
  });

  it("自分のノート・全体オフ・カラムオフのどれでも null", () => {
    expect(speechTextForColumnNote(note(), { isOwn: true, globalEnabled: true, column: col })).toBeNull();
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: false, column: col })).toBeNull();
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: true, column: DEFAULT_COLUMN_SPEECH })).toBeNull();
  });

  it("カラムの speakAuthor に従う", () => {
    const c = { ...col, speakAuthor: false };
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: true, column: c })).toBe("こんにちは");
  });

  it("既定のカラム設定は、オフ・投稿者名あり・上限5・声は継承", () => {
    expect(DEFAULT_COLUMN_SPEECH).toEqual({ enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" });
  });
});
```

Run: `cd frontend && npx vitest run src/lib/speechText.test.ts`
Expected: FAIL(モジュールが無い)。

- [ ] **Step 2: 実装する**

`frontend/src/lib/speechText.ts`:

```ts
// 読み上げ(Issue #625)の文の整形。ノートから「読み上げる文字列」を作る純粋な関数だけを持つ。
// 合成・キュー・再生は Rust(src-tauri/src/tts/)の担当で、ここは MFM をプレーンな文にするところまで。
import { parse, parseSimple, type MfmNode } from "mfm-js";
import type { ColumnSpeech, Note, User } from "../bindings/tauri.gen";

/// 1件の読み上げ本文の最大文字数(コードポイント)。超えたら切って「以下略」を付ける。
export const MAX_SPEECH_CHARS = 120;
const ELLIPSIS = "、以下略";

/// カラムの読み上げ設定の既定値(Rust の `ColumnSpeech::default()` と揃える)。
export const DEFAULT_COLUMN_SPEECH: ColumnSpeech = {
  enabled: false,
  speakAuthor: true,
  queueLimit: 5,
  voiceId: "",
};

/// MFM のノードを、読み上げ用のプレーンな文字列にする。読めないもの(絵文字・数式・検索)は空にする。
function nodeText(n: MfmNode): string {
  switch (n.type) {
    case "text":
      return n.props.text;
    case "bold":
    case "italic":
    case "small":
    case "strike":
    case "center":
    case "quote":
    case "plain":
    case "fn":
      return plain(n.children);
    case "mention":
      return n.props.username;
    case "hashtag":
      return n.props.hashtag;
    case "url":
      return "URL";
    case "link": {
      const label = plain(n.children).trim();
      return label || "URL";
    }
    case "inlineCode":
    case "blockCode":
      return "コード";
    default:
      // emojiCode / unicodeEmoji / mathInline / mathBlock / search
      return "";
  }
}

function plain(nodes: MfmNode[]): string {
  return nodes.map(nodeText).join("");
}

/// 空白・改行を読みやすく整える。改行は読点にする(句読点の直後の改行は読点を足さない)。
function normalize(s: string): string {
  return s
    .replace(/([。．.!?！？、,，])?[ \t　]*\n[\s　]*/g, (_m, p: string | undefined) => p ?? "、")
    .replace(/[ \t　]+/g, " ")
    .replace(/、{2,}/g, "、")
    .replace(/^[\s、]+|[\s、]+$/g, "");
}

/// 上限(コードポイント単位)を超えたら切って「以下略」を付ける。サロゲートペアを割らない。
function truncate(s: string): string {
  const chars = Array.from(s);
  if (chars.length <= MAX_SPEECH_CHARS) return s;
  return chars.slice(0, MAX_SPEECH_CHARS).join("") + ELLIPSIS;
}

/// 読み上げの本文。純粋なリノートや、読む中身がないノートは null。
function bodyText(note: Note): string | null {
  // 純粋なリノート(本文も CW もない)は、元のノートが別に流れてくるので読まない。
  if (note.renoteId && !note.text && !note.cw) return null;
  // CW があれば CW だけ(本文は隠す意図があるので、読み上げて裏切らない)。
  const source = note.cw && note.cw.trim() ? note.cw : note.text;
  if (!source) return null;
  const nodes = note.cw && note.cw.trim() ? parseSimple(source) : parse(source);
  const text = normalize(plain(nodes));
  return text ? truncate(text) : null;
}

/// 投稿者の読み名。表示名のカスタム絵文字を除き、空なら username。
function authorName(user: User): string {
  const name = user.name ? normalize(plain(parseSimple(user.name))) : "";
  return name || user.username;
}

/// ノート1件の読み上げ文。読まないときは null。`speakAuthor` がオンなら `{表示名}さん、` を前置きする。
export function buildSpeechText(note: Note, opts: { speakAuthor: boolean }): string | null {
  const body = bodyText(note);
  if (body === null) return null;
  return opts.speakAuthor ? `${authorName(note.user)}さん、${body}` : body;
}

/// 新着ノートを読み上げるか判定し、読むなら読み上げ文を返す(読まないなら null)。
/// 自分のノート・全体オフ・カラムオフのときは読まない。重複排除(`speak:<noteId>`)は呼び出し側(store)が行う。
export function speechTextForColumnNote(
  note: Note,
  ctx: { isOwn: boolean; globalEnabled: boolean; column: ColumnSpeech },
): string | null {
  if (ctx.isOwn || !ctx.globalEnabled || !ctx.column.enabled) return null;
  return buildSpeechText(note, { speakAuthor: ctx.column.speakAuthor });
}
```

Run: `cd frontend && npx vitest run src/lib/speechText.test.ts`
Expected: PASS。失敗が出た場合は、失敗しているケースの `mfm-js` の実際の AST(`parse` の出力)を `node -e` で確認してから `nodeText` / `normalize` を直す(テストの期待を変えない)。たとえば `"@bob@example.com"` が `mention` ノード(`props.username = "bob"`、`props.host = "example.com"`)になること、`"\\(x^2\\)"` が `mathInline` になること、`$[tada ほげ]` が `fn` になることを確認する。

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/speechText.ts frontend/src/lib/speechText.test.ts
git commit -m "feat(tts): 読み上げ文の整形を追加(Issue #625)"
```

---

### Task 9: ストアとIPCの配線(発火)

**Files:**
- Modify: `frontend/src/lib/ipc.ts`(`speakNote` ラッパー)
- Modify: `frontend/src/lib/store.svelte.ts`(`TabView.speech`、`app.speech`、`app.speechStatus`、`setSpeechSettings`、`setColumnSpeech`、起動時の読み込み、イベント購読、`columnNote` の発火)
- Modify(フィクスチャに `speech` を足す): `frontend/src/lib/store.svelte.test.ts`、`store.svelte.needsLogin.test.ts`、`store.svelte.resumeColumns.test.ts`、`frontend/src/ui/AddColumnModal.test.ts`、`frontend/src/ui/Column.test.ts`
- Test: `frontend/src/lib/ipc.speakNote.test.ts`、`frontend/src/lib/store.speech.test.ts`

**Interfaces:**
- Consumes: Task 7 のバインディング(`commands.speakNote(columnId, noteId, text)`、`commands.getSpeechSettings()`、`commands.setSpeechSettings(settings)`、`commands.setColumnSpeech(columnId, speech)`、`events.speechStatusChanged`)、Task 8 の `speechTextForColumnNote` / `DEFAULT_COLUMN_SPEECH`
- Produces:
  - `ipc.speakNote(columnId: string, noteId: string, text: string, onError?: (e: unknown) => void): void`(例外を投げない)
  - `TabView.speech: ColumnSpeech`
  - `app.speech: SpeechSettings`、`app.speechStatus: SpeechStatus`
  - `app.setSpeechSettings(s: SpeechSettings): Promise<void>`、`app.setColumnSpeech(tabId: string, speech: ColumnSpeech): Promise<void>`

- [ ] **Step 1: `speakNote` ラッパーのテストを書く(失敗させる)**

`frontend/src/lib/ipc.speakNote.test.ts`(`ipc.playNotifySound.test.ts` の構成に倣う。先頭のモック宣言は同ファイルのものを写す):

```ts
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { commands, speakNote } = await import("./ipc");

afterEach(() => vi.restoreAllMocks());

describe("speakNote", () => {
  it("columnId / noteId / text をそのまま speakNote コマンドへ渡す", async () => {
    const spy = vi.spyOn(commands, "speakNote").mockResolvedValue({ status: "ok", data: null });
    speakNote("c1", "n1", "こんにちは");
    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith("c1", "n1", "こんにちは"));
  });

  it("コマンドが error を返しても例外を投げず、onError に渡す", async () => {
    vi.spyOn(commands, "speakNote").mockResolvedValue({ status: "error", error: { kind: "invalid", message: "SPEECH_NO_VOICE" } });
    const onError = vi.fn();
    expect(() => speakNote("c1", "n1", "x", onError)).not.toThrow();
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());
  });

  it("invoke 自体が reject しても unhandled rejection にならない", async () => {
    vi.spyOn(commands, "speakNote").mockRejectedValue(new Error("boom"));
    const onError = vi.fn();
    speakNote("c1", "n1", "x", onError);
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());
  });
});
```

Run: `cd frontend && npx vitest run src/lib/ipc.speakNote.test.ts`
Expected: FAIL(`speakNote` が無い)。

- [ ] **Step 2: ラッパーを実装する**

`frontend/src/lib/ipc.ts` の `vibrate` の直前(`playNotifySound` の後)に追加する。

```ts
/// 読み上げを依頼する(Issue #625)。`text` は `speechText.ts` が整形した文。fire-and-forget で、
/// 失敗(エンジン未設定・キューの拒否・IPC 自体の失敗)はノートの表示を妨げないよう握りつぶし、理由だけ onError に渡す。
export function speakNote(columnId: string, noteId: string, text: string, onError?: (e: unknown) => void): void {
  void unwrap(commands.speakNote(columnId, noteId, text)).catch((e) => onError?.(e));
}
```

Run: `cd frontend && npx vitest run src/lib/ipc.speakNote.test.ts`
Expected: PASS。

- [ ] **Step 3: 発火の判定のテストを書く(store 側)**

`frontend/src/lib/store.speech.test.ts`(`store.svelte.test.ts` と同じモック構成。リスナー本体は Tauri のイベントで呼ばれるので、ここでは「store が持つ状態と設定の更新」を検証し、発火の判定そのものは Task 8 の `speechTextForColumnNote` のテストが担う):

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { app } = await import("./store.svelte");

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue({ status: "ok", data: null });
});
afterEach(() => vi.restoreAllMocks());

describe("読み上げの設定(store)", () => {
  it("既定は、全体オフ・外部エンジン・URL なし・状態は unknown", () => {
    expect(app.speech).toMatchObject({ enabled: false, engine: "external", externalUrl: "", speedScale: 1, volume: 1 });
    expect(app.speechStatus).toEqual({ type: "unknown" });
  });

  it("setSpeechSettings は保存した結果(正規化後)を app.speech に反映する", async () => {
    const saved = { enabled: true, engine: "external" as const, externalUrl: "http://h:1", defaultVoiceId: "3", speedScale: 1.2, volume: 1 };
    invokeMock.mockResolvedValueOnce({ status: "ok", data: saved });
    await app.setSpeechSettings({ ...saved, externalUrl: "http://h:1/" });
    expect(invokeMock).toHaveBeenCalledWith("set_speech_settings", { settings: { ...saved, externalUrl: "http://h:1/" } });
    expect(app.speech).toEqual(saved);
  });

  it("setSpeechSettings が拒否されたら、app.speech は変えず、エラーを投げる", async () => {
    const before = { ...app.speech };
    invokeMock.mockResolvedValueOnce({ status: "error", error: { kind: "invalid", message: "URL の形式が正しくありません" } });
    await expect(app.setSpeechSettings({ ...before, externalUrl: "ftp://x" })).rejects.toBeTruthy();
    expect(app.speech).toEqual(before);
  });

  it("setColumnSpeech は該当タブの speech を更新して、コマンドを呼ぶ", async () => {
    const tab = { id: "t1", speech: { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" } } as never;
    app.groups = [{ id: "g", width: 300, auto: false, tabs: [tab], activeTabId: "t1" }] as never;
    const next = { enabled: true, speakAuthor: false, queueLimit: 3, voiceId: "9" };
    await app.setColumnSpeech("t1", next);
    expect((app.groups[0].tabs[0] as { speech: unknown }).speech).toEqual(next);
    expect(invokeMock).toHaveBeenCalledWith("set_column_speech", { columnId: "t1", speech: next });
  });
});
```

Run: `cd frontend && npx vitest run src/lib/store.speech.test.ts`
Expected: FAIL(`app.speech` が無い)。

- [ ] **Step 4: store を実装する**

`frontend/src/lib/store.svelte.ts` を編集する。

(a) import に追加する(既存の `import { commands, events, playNotifySound, unwrap, … } from "./ipc";` に `speakNote` を足し、型は `../bindings/tauri.gen` の既存の import 行に `ColumnSpeech, SpeechSettings, SpeechStatus` を足す。`./speechText` の import を足す)。

```ts
import { DEFAULT_COLUMN_SPEECH, speechTextForColumnNote } from "./speechText";
```

(b) `TabView` の `notifySoundChoice: string;` の次に追加する。

```ts
  /// このタブの読み上げ設定(Issue #625)。全体(`app.speech.enabled`)と両方オンのときだけ読み上げる。
  speech: ColumnSpeech;
```

(c) `#makeTab` の `notifySoundChoice: opened.column.notifySoundChoice,` の次に追加する(古いモックなどで `speech` が無くても落ちないよう、既定値にフォールバックする)。

```ts
      speech: opened.column.speech ?? { ...DEFAULT_COLUMN_SPEECH },
```

(d) `notify = $state<NotifyConfig>(…)`(176行目)の次に追加する。

```ts
  /// 読み上げの全体設定(Issue #625)。起動時に `getSpeechSettings` で上書きする。
  speech = $state<SpeechSettings>({
    enabled: false,
    engine: "external",
    externalUrl: "",
    defaultVoiceId: "",
    speedScale: 1,
    volume: 1,
  });
  /// 読み上げの状態(エンジンに繋がらない等)。`SpeechStatusChanged` で更新する。
  speechStatus = $state<SpeechStatus>({ type: "unknown" });
```

(e) `boot()` の `prewarmSoundChoice(this.notify.soundChoice ?? "");` の次に追加する(IPC のモックが `data: null` を返すテストでも落ちないよう、null を弾く)。

```ts
      const speech = await unwrap(commands.getSpeechSettings());
      if (speech) this.speech = speech;
      const speechStatus = await unwrap(commands.getSpeechStatus());
      if (speechStatus) this.speechStatus = speechStatus;
```

(f) `setColumnNotify` の直後に追加する。

```ts
  /// タブ単位の読み上げ設定を変更する(Issue #625)。ストリームは張り直さない。
  async setColumnSpeech(tabId: string, speech: ColumnSpeech) {
    const tab = this.#findTab(tabId);
    if (tab) tab.speech = speech;
    try {
      await unwrap(commands.setColumnSpeech(tabId, speech));
    } catch (e) {
      this.#failModal(e);
    }
  }

  /// 読み上げの全体設定を保存する。Rust が検証・整形した値を返すので、それを反映する。拒否されたら例外を投げる
  /// (設定画面が自分で理由を表示する)。
  async setSpeechSettings(settings: SpeechSettings) {
    this.speech = await unwrap(commands.setSpeechSettings(settings));
  }
```

(g) `columnNote` リスナー(1010行目付近)の、通知音の発火ブロックの直後に追加する。

```ts
          if (wantsDesktop) void this.#osNotifyNote(tab, e.payload.note);
          if (wantsSound) this.#playTabSound(tab);
        }
        // 読み上げ(Issue #625)。通知音とは別の重複排除の枠(`speak:`)を使う: 通知音が枠を消費しても読み上げは落とさない。
        const speechText = speechTextForColumnNote(e.payload.note, {
          isOwn: !!isOwn,
          globalEnabled: this.speech.enabled,
          column: tab.speech,
        });
        if (speechText && this.#markNotified(`speak:${e.payload.note.id}`)) {
          speakNote(tab.id, e.payload.note.id, speechText, (err) => this.#logDebug(`読み上げの依頼に失敗: ${String(err)}`));
        }
      }),
```

(元の `if (wantsDesktop) …` から `}` までの3行を、上のブロックで置き換える。`      }),` の閉じ括弧は元のまま残る。)

(h) イベント購読を足す。`await events.columnNotificationGapFill.listen((e) => {` を含む `this.#unlisten.push(` ブロックの**直前**に追加する。

```ts
    this.#unlisten.push(
      await events.speechStatusChanged.listen((e) => {
        this.speechStatus = e.payload.status;
        if (e.payload.status.type === "error") {
          this.#log("warn", `読み上げ: エンジンに接続できません(${e.payload.status.message})`);
        }
      }),
    );
```

`#log` の第1引数に `"warn"` が使えるかは、既存の呼び出し(`this.#log("info", …)`、`"success"`)の型を確認して、無ければ既存の最も近いレベル(`"error"`)にする。

Run: `cd frontend && npx vitest run src/lib/store.speech.test.ts`
Expected: PASS。

- [ ] **Step 5: 既存のテストのフィクスチャを更新する**

`speech` を持たない `TabView` / `Column` のフィクスチャが型エラーになる。`notifySoundChoice:` を含む既存のテストの各フィクスチャに、`speech` を足す。

```bash
cd frontend && grep -rn "notifySoundChoice" src --include='*.test.ts' | grep -v "toMatchObject\|expect"
```

見つかった各オブジェクトリテラルの `notifySoundChoice: "…",` の次の行に、次を足す。

```ts
    speech: { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" },
```

(`ui/AddColumnModal.test.ts` の `makeTab`、`ui/Column.test.ts`、`lib/store.svelte.test.ts`、`lib/store.svelte.needsLogin.test.ts`、`lib/store.svelte.resumeColumns.test.ts` の該当箇所。)

Run: `cd frontend && pnpm check`
Expected: エラーなし。残っていれば、指摘された箇所に同じ行を足す。

Run: `cd frontend && pnpm test 2>&1 | tail -15`
Expected: 既存を含めて PASS。

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(tts): 新着ノートの読み上げ発火とストアの設定を追加(Issue #625)"
```

---

### Task 10: UI(設定画面「読み上げ」とカラム設定)

**Files:**
- Create: `frontend/src/ui/settings/SpeechSection.svelte`、`frontend/src/ui/settings/SpeechSection.test.ts`
- Modify: `frontend/src/ui/Settings.svelte`(セクションの登録)
- Modify: `frontend/src/ui/AddColumnModal.svelte`、`frontend/src/ui/AddColumnModal.test.ts`

**Interfaces:**
- Consumes: Task 9 の `app.speech` / `app.speechStatus` / `app.setSpeechSettings` / `app.setColumnSpeech`、Task 7 のコマンド `listSpeechVoices` / `testSpeech` / `stopSpeech`、Task 1 の記録(Android の平文 HTTP)
- Produces: `data-testid` — 設定: `speech-enabled`、`speech-url`、`speech-connect-test`、`speech-connect-result`、`speech-voice`、`speech-speed`、`speech-volume`、`speech-try`、`speech-stop`、`speech-status`、`speech-credit`。カラム: `speech-column-enabled`、`speech-column-author`、`speech-column-limit`、`speech-column-voice`。

- [ ] **Step 1: 設定画面のテストを書く(失敗させる)**

`frontend/src/ui/settings/SpeechSection.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";
import { commands } from "../../lib/ipc";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: SpeechSection } = await import("./SpeechSection.svelte");

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const err = (message: string) => ({ status: "error" as const, error: { kind: "invalid" as const, message } });

beforeEach(() => {
  app.speech = { enabled: false, engine: "external", externalUrl: "", defaultVoiceId: "", speedScale: 1, volume: 1 };
  app.speechStatus = { type: "unknown" };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SpeechSection", () => {
  it("既定はオフ・URL なし。規約への注意書きがある", () => {
    const { getByTestId } = render(SpeechSection);
    expect((getByTestId("speech-enabled") as HTMLInputElement).checked).toBe(false);
    expect((getByTestId("speech-url") as HTMLInputElement).value).toBe("");
    expect(getByTestId("speech-credit").textContent).toContain("クレジット");
  });

  it("接続テストは入力欄の URL(未保存)で声の一覧を取得し、選択肢に反映する", async () => {
    const spy = vi.spyOn(commands, "listSpeechVoices").mockResolvedValue(
      ok([
        { id: "3", label: "ずんだもん（ノーマル）" },
        { id: "2", label: "四国めたん（ノーマル）" },
      ]),
    );
    const { getByTestId, findByTestId } = render(SpeechSection);
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "http://192.168.1.10:50021" } });
    await fireEvent.click(getByTestId("speech-connect-test"));

    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith("http://192.168.1.10:50021"));
    expect((await findByTestId("speech-connect-result")).textContent).toContain("2");
    const select = getByTestId("speech-voice") as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.textContent?.trim())).toContain("ずんだもん（ノーマル）");
    expect(select.value, "声が未選択なら先頭を選ぶ").toBe("3");
  });

  it("接続できなかったら、理由を表示する", async () => {
    vi.spyOn(commands, "listSpeechVoices").mockResolvedValue(err("SPEECH_ENGINE_UNREACHABLE: connect"));
    const { getByTestId, findByTestId } = render(SpeechSection);
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "http://h:1" } });
    await fireEvent.click(getByTestId("speech-connect-test"));
    expect((await findByTestId("speech-connect-result")).textContent).toContain("接続できませんでした");
  });

  it("保存すると、入力した値を setSpeechSettings に渡す(URL は前後の空白を除く)", async () => {
    const spy = vi.spyOn(app, "setSpeechSettings").mockResolvedValue(undefined);
    const { getByTestId, getByText } = render(SpeechSection);
    await fireEvent.click(getByTestId("speech-enabled"));
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "  http://h:1  " } });
    await fireEvent.input(getByTestId("speech-speed"), { target: { value: "1.5" } });
    await fireEvent.click(getByText("保存"));
    await vi.waitFor(() =>
      expect(spy).toHaveBeenCalledWith({
        enabled: true,
        engine: "external",
        externalUrl: "http://h:1",
        defaultVoiceId: "",
        speedScale: 1.5,
        volume: 1,
      }),
    );
  });

  it("保存が拒否されたら、理由を表示する", async () => {
    vi.spyOn(app, "setSpeechSettings").mockRejectedValue({ kind: "invalid", message: "URL の形式が正しくありません" });
    const { getByTestId, getByText, findByText } = render(SpeechSection);
    await fireEvent.input(getByTestId("speech-url"), { target: { value: "ftp://x" } });
    await fireEvent.click(getByText("保存"));
    expect(await findByText(/URL の形式が正しくありません/)).toBeTruthy();
  });

  it("試し聞きと停止は、それぞれのコマンドを呼ぶ", async () => {
    const tryIt = vi.spyOn(commands, "testSpeech").mockResolvedValue(ok(null));
    const stop = vi.spyOn(commands, "stopSpeech").mockResolvedValue(ok(null));
    const { getByTestId } = render(SpeechSection);
    await fireEvent.click(getByTestId("speech-try"));
    await vi.waitFor(() => expect(tryIt).toHaveBeenCalled());
    await fireEvent.click(getByTestId("speech-stop"));
    await vi.waitFor(() => expect(stop).toHaveBeenCalled());
  });

  it("エンジンに繋がらない状態を表示する", () => {
    app.speechStatus = { type: "error", message: "SPEECH_ENGINE_UNREACHABLE: connect" };
    const { getByTestId } = render(SpeechSection);
    expect(getByTestId("speech-status").textContent).toContain("接続できません");
  });
});
```

Run: `cd frontend && npx vitest run src/ui/settings/SpeechSection.test.ts`
Expected: FAIL(コンポーネントが無い)。

- [ ] **Step 2: 設定画面を実装する**

`frontend/src/ui/settings/SpeechSection.svelte`:

```svelte
<script lang="ts">
  import SaveBar from "./SaveBar.svelte";
  import { Button } from "$lib/components/ui/button";
  import { app } from "../../lib/store.svelte";
  import { commands, formatError, unwrap, type Voice } from "../../lib/ipc";
  import { isMobilePlatform } from "../../lib/platform";

  // 読み上げの全体設定(Issue #625)。外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)に接続する。
  let enabled = $state(app.speech.enabled);
  let externalUrl = $state(app.speech.externalUrl);
  let defaultVoiceId = $state(app.speech.defaultVoiceId);
  let speedScale = $state(app.speech.speedScale);
  let volume = $state(app.speech.volume);

  let voices = $state<Voice[]>([]);
  let connecting = $state(false);
  let connectResult = $state<string | null>(null);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);
  let actionErr = $state<string | null>(null);

  const statusText = $derived.by(() => {
    const s = app.speechStatus;
    if (s.type === "ok") return "エンジンに接続できています";
    if (s.type === "error") return `エンジンに接続できません(${s.message})`;
    return "";
  });

  /// 入力欄の URL(未保存でよい)でエンジンに接続し、選べる声を取得する。
  async function connectTest() {
    connectResult = null;
    connecting = true;
    try {
      voices = await unwrap(commands.listSpeechVoices(externalUrl.trim()));
      connectResult = `接続できました(声 ${voices.length} 件)`;
      if (!voices.some((v) => v.id === defaultVoiceId)) defaultVoiceId = voices[0]?.id ?? "";
    } catch (e) {
      voices = [];
      connectResult = `接続できませんでした: ${formatError(e)}`;
    } finally {
      connecting = false;
    }
  }

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      await app.setSpeechSettings({
        enabled,
        engine: "external",
        externalUrl: externalUrl.trim(),
        defaultVoiceId,
        speedScale: Number(speedScale),
        volume: Number(volume),
      });
      // Rust が整えた値(末尾のスラッシュ除去など)を入力欄に戻す。
      externalUrl = app.speech.externalUrl;
      saved = true;
    } catch (e) {
      err = formatError(e);
    } finally {
      busy = false;
    }
  }

  /// 保存済みの設定で、固定の文を読み上げる。
  async function trySpeak() {
    actionErr = null;
    try {
      await unwrap(commands.testSpeech("こんにちは。読み上げのテストです。"));
    } catch (e) {
      actionErr = formatError(e);
    }
  }

  async function stop() {
    actionErr = null;
    try {
      await unwrap(commands.stopSpeech());
    } catch (e) {
      actionErr = formatError(e);
    }
  }

  const inputClass = "rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground";
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">読み上げ</h3>

<label class="mb-2 flex items-center gap-2 text-sm">
  <input type="checkbox" data-testid="speech-enabled" bind:checked={enabled} /> 読み上げを使う(全体スイッチ)
</label>
<p class="my-1 mb-4 text-xs text-muted-foreground">
  読み上げをオンにしたカラム(カラムをダブルクリック → 「このタブの読み上げ」)に届いた新着ノートを、順番に読み上げます。
  ここは全カラム共通のマスタースイッチです(両方オンのときだけ読み上げます)。
</p>

<label class="mb-2 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">音声合成エンジンの URL(VOICEVOX ENGINE / AivisSpeech Engine)</span>
  <input class={inputClass} data-testid="speech-url" placeholder="http://192.168.1.10:50021" bind:value={externalUrl} />
</label>
<div class="mb-2 flex items-center gap-2 text-sm">
  <Button variant="outline" disabled={connecting || !externalUrl.trim()} onclick={connectTest} data-testid="speech-connect-test">
    {connecting ? "接続中…" : "接続テスト"}
  </Button>
  {#if connectResult}<span class="break-words text-xs text-muted-foreground" data-testid="speech-connect-result">{connectResult}</span>{/if}
</div>
{#if statusText}
  <p class="my-1 mb-3 text-xs {app.speechStatus.type === 'error' ? 'text-destructive' : 'text-muted-foreground'}" data-testid="speech-status">{statusText}</p>
{/if}
<p class="my-1 mb-4 text-xs text-muted-foreground">
  エンジンは別途起動してください。LAN 内のサーバーに接続する場合は、そのアドレスを指定します。
  {#if isMobilePlatform}
    Android では、端末の設定によって LAN 内の平文(http://)のサーバーに接続できないことがあります。接続テストが失敗する場合は、https:// の URL を指定してください。
  {/if}
</p>

<label class="mb-2 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">既定の声(カラムごとに上書きできます)</span>
  <select class={inputClass} data-testid="speech-voice" bind:value={defaultVoiceId}>
    {#if voices.length === 0}
      <option value={defaultVoiceId}>{defaultVoiceId ? `現在の声: ${defaultVoiceId}` : "接続テストで声を取得してください"}</option>
    {:else}
      {#each voices as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
    {/if}
  </select>
</label>

<label class="mb-2 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">話速({speedScale})</span>
  <input type="range" min="0.5" max="2" step="0.1" data-testid="speech-speed" bind:value={speedScale} />
</label>
<label class="mb-3 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">音量({volume})</span>
  <input type="range" min="0.1" max="2" step="0.1" data-testid="speech-volume" bind:value={volume} />
</label>

<div class="mb-2 flex items-center gap-2 text-sm">
  <Button variant="outline" onclick={trySpeak} data-testid="speech-try">試し聞き</Button>
  <Button variant="outline" onclick={stop} data-testid="speech-stop">停止</Button>
  <span class="text-xs text-muted-foreground">試し聞きは保存済みの設定で読み上げます。</span>
</div>
{#if actionErr}<p class="my-1 break-words text-sm text-destructive">{actionErr}</p>{/if}

<p class="my-3 text-xs text-muted-foreground" data-testid="speech-credit">
  VOICEVOX 系の声を使った音声を公開する場合は、「VOICEVOX:キャラクター名」のクレジット表記と、各キャラクターの
  利用規約の遵守が必要です(VOICEVOX 音声モデル利用規約)。利用する声の規約を確認してください。
</p>

<SaveBar {busy} {saved} {err} onsave={save} />
```

Android の注意書き(`{#if isMobilePlatform}` のブロック)は、Task 1 の(b)の結論がどちらでも成り立つ「〜ことがあります」の言い方にしてある。Task 1 で「LAN の http に接続できる」と確認できた場合は、このブロックを削除してよい(実機の確認は Task 11 Step 5 でも行う)。

`Settings.svelte` に登録する。`type Section =` の `| "notify"` の次に `| "speech"` を足し、ナビの配列の `{ id: "notify" as const, label: "通知" },` の次に `{ id: "speech" as const, label: "読み上げ" },` を足し、`{:else if active === "notify"}\n    <NotifySection />` の次に次を足す。

```svelte
  {:else if active === "speech"}
    <SpeechSection />
```

import に `import SpeechSection from "./settings/SpeechSection.svelte";` を足す。

Run: `cd frontend && npx vitest run src/ui/settings/SpeechSection.test.ts`
Expected: PASS。

- [ ] **Step 3: カラム設定のテストを書く(失敗させる)**

`frontend/src/ui/AddColumnModal.test.ts` の末尾に追加する(既存の `setupEdit` 流儀。`makeTab`・`invokeMock` は同ファイルのもの)。

```ts
describe("AddColumnModal の読み上げ(Issue #625)", () => {
  function setupEdit(speech = { enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" }) {
    const tab = makeTab({ type: "home" }, { kind: "keywords", value: [] });
    tab.speech = speech;
    app.groups = [{ id: "group1", width: 400, auto: false, tabs: [tab], activeTabId: tab.id }];
    return tab;
  }
  const speechCall = () => invokeMock.mock.calls.find((c) => c[0] === "set_column_speech");

  it("全体の読み上げがオフのときは、トグルが無効で案内が出る", () => {
    app.speech = { ...app.speech, enabled: false };
    const tab = setupEdit();
    const { getByTestId, getByText } = render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    expect((getByTestId("speech-column-enabled") as HTMLInputElement).disabled).toBe(true);
    expect(getByText(/設定で読み上げをオンにしてください/)).toBeTruthy();
  });

  it("全体がオンで、トグルをオンにして保存すると set_column_speech を呼ぶ", async () => {
    app.speech = { ...app.speech, enabled: true };
    const tab = setupEdit();
    const onclose = vi.fn();
    const { getByTestId } = render(AddColumnModal, { onclose, groupId: null, editTab: tab });
    await fireEvent.click(getByTestId("speech-column-enabled"));
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(speechCall()?.[1]).toMatchObject({ columnId: "tab1", speech: { enabled: true, speakAuthor: true, queueLimit: 5, voiceId: "" } });
  });

  it("変更がなければ set_column_speech を呼ばない", async () => {
    app.speech = { ...app.speech, enabled: true };
    const tab = setupEdit();
    const onclose = vi.fn();
    const { getByTestId } = render(AddColumnModal, { onclose, groupId: null, editTab: tab });
    await fireEvent.click(getByTestId("add-column-submit"));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(speechCall()).toBeUndefined();
  });

  it("通知カラムには読み上げの設定を出さない", () => {
    app.speech = { ...app.speech, enabled: true };
    const tab = makeTab({ type: "notifications" }, { kind: "keywords", value: [] });
    app.groups = [{ id: "group1", width: 400, auto: false, tabs: [tab], activeTabId: tab.id }];
    const { queryByTestId } = render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    expect(queryByTestId("speech-column-enabled")).toBeNull();
  });
});
```

Run: `cd frontend && npx vitest run src/ui/AddColumnModal.test.ts`
Expected: 新しい4件が FAIL(既存は PASS のまま)。

- [ ] **Step 4: カラム設定を実装する**

`frontend/src/ui/AddColumnModal.svelte` を編集する。

(a) `<script>` の `let soundInvalid = $state(false);` の次に追加する。

```ts
  // 読み上げ(Issue #625)。通知カラムは対象外(ノートのタイムラインだけを読む)。
  let speechEnabled = $state(edit?.speech.enabled ?? false);
  let speakAuthor = $state(edit?.speech.speakAuthor ?? true);
  let queueLimit = $state(edit?.speech.queueLimit ?? 5);
  let speechVoiceId = $state(edit?.speech.voiceId ?? "");
  let speechVoices = $state<Voice[]>([]);
  const speechShown = $derived(sourceType !== "notifications");
  const speechSettings = $derived<ColumnSpeech>({ enabled: speechEnabled, speakAuthor, queueLimit: Number(queueLimit) || 5, voiceId: speechVoiceId });

  // 声の選択肢は、エンジンに繋がるときだけ取得する(失敗しても「既定の声を使う」だけで使える)。
  $effect(() => {
    if (!speechEnabled || !app.speech.externalUrl) return;
    let cancelled = false;
    commands.listSpeechVoices(app.speech.externalUrl).then((r) => {
      if (!cancelled && r.status === "ok") speechVoices = r.data;
    }).catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  /// 読み上げの設定が変わっていれば保存する(通知設定の保存の直後に呼ぶ)。
  async function applySpeech(tabId: string, current: ColumnSpeech) {
    const next = speechSettings;
    if (
      next.enabled !== current.enabled ||
      next.speakAuthor !== current.speakAuthor ||
      next.queueLimit !== current.queueLimit ||
      next.voiceId !== current.voiceId
    ) {
      await app.setColumnSpeech(tabId, next);
    }
  }
```

`import` に `import { commands, type ColumnSpeech, type Voice } from "../lib/ipc";`(既存の import に合わせて追加。`app` は既に import 済み)を足す。

(b) `submit()` の4箇所の保存を更新する。各 `await app.setColumnNotify(…)` の直後に `applySpeech` を足す。編集の2箇所は `edit.speech`、新規作成の2箇所は `tab.speech`(`addColumn` が返す新しいタブの既定値)を渡す。

```ts
          await app.setColumnNotify(edit.id, notifyDesktop, notifySound, effectiveSoundChoice);
          await applySpeech(edit.id, edit.speech);
```

新規作成側(`if (notifyDesktop !== tab.notifyDesktop || …) { … }` の後ろ。通知設定が既定と同じでも読み上げは別に保存する):

```ts
          await applySpeech(tab.id, tab.speech);
```

(TQL の枝(`uiMode === "expert"`)と通常の枝の両方に、同じ2種類の呼び出しを足す。)

(c) テンプレートに、読み上げのブロックを snippet として定義する。`<Modal` の**直前**に追加する。

```svelte
{#snippet speechFields(showDetails: boolean)}
  {#if speechShown}
    <div class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">このタブの読み上げ</span>
      <label class="flex items-center gap-1.5 text-sm">
        <input type="checkbox" data-testid="speech-column-enabled" bind:checked={speechEnabled} disabled={!app.speech.enabled} /> 新着ノートを読み上げる
      </label>
      {#if !app.speech.enabled}
        <p class="m-0 text-xs text-muted-foreground">設定で読み上げをオンにしてください(設定 → 読み上げ)。</p>
      {/if}
      {#if speechEnabled && showDetails}
        <label class="flex items-center gap-1.5 text-sm">
          <input type="checkbox" data-testid="speech-column-author" bind:checked={speakAuthor} /> 投稿者名を読む
        </label>
        <label class="flex items-center gap-1.5 text-sm">
          待ち件数の上限
          <input
            type="number" min="1" max="20" data-testid="speech-column-limit"
            class="w-16 rounded-lg border border-border bg-muted px-2 py-1 font-[inherit] text-foreground"
            bind:value={queueLimit}
          />
        </label>
        <label class="flex flex-col gap-1 text-sm">
          <span class="text-muted-foreground">声</span>
          <select
            data-testid="speech-column-voice"
            class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            bind:value={speechVoiceId}
          >
            <option value="">既定の声を使う</option>
            {#each speechVoices as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
            {#if speechVoiceId && !speechVoices.some((v) => v.id === speechVoiceId)}
              <option value={speechVoiceId}>現在の声: {speechVoiceId}</option>
            {/if}
          </select>
        </label>
      {/if}
    </div>
  {/if}
{/snippet}
```

(d) 2つの通知ブロックの直後に `{@render speechFields(…)}` を足す。ガイドモードのブロック(`{#if guidedNotifyShown} … {:else} … {/if}` の `{/if}` の直後)に:

```svelte
  {@render speechFields(false)}
```

エキスパートのブロック(`{#if uiMode === "expert"} … ストリーミング対応のソースに新着があれば発火します。… </p>` の直後、`{/if}` の前)に:

```svelte
    {@render speechFields(true)}
```

Run: `cd frontend && npx vitest run src/ui/AddColumnModal.test.ts`
Expected: PASS(既存 + 新規4件)。

- [ ] **Step 5: 型・全テスト・スタイルガイドの確認**

Run: `cd frontend && pnpm check && pnpm test 2>&1 | tail -10`
Expected: エラーなし・全テスト PASS。

`docs/design/style-guide.md` に反する値(`rounded-[…]`、`text-[…px]` など)を足していないことを確認する。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git diff -U0 frontend/src/ui | grep -nE "rounded-\[|text-\[[0-9]+px\]" || echo "OK: 一回限りの値なし"
```

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui
git commit -m "feat(tts): 読み上げの設定画面とカラム設定を追加(Issue #625)"
```

---

### Task 11: ドキュメントと最終確認

**Files:**
- Modify: `docs/guide/user-guide.md`、`CLAUDE.md`
- Modify: `docs/superpowers/plans/2026-11-19-issue-625-spike-notes.md`((c) の結果)

**Interfaces:**
- Consumes: Task 1〜10 のすべて
- Produces: ユーザー向けの説明、`tts/` の開発者向けの説明、検証の記録

- [ ] **Step 1: ユーザーガイドに節を足す**

`docs/guide/user-guide.md` の通知の節(`grep -n "通知" docs/guide/user-guide.md` で見出しを探す)の直後に、次の節を足す。

```markdown
## 読み上げ

指定したカラムに届いた新着ノートを、音声合成で読み上げます。音声合成には、別途起動した **VOICEVOX ENGINE** または **AivisSpeech Engine**(VOICEVOX 互換 API)を使います。

### 準備

1. エンジンを起動します(例: `docker run -d -p 127.0.0.1:50021:50021 voicevox/voicevox_engine:cpu-latest`)。
2. 設定 → 読み上げ で、「読み上げを使う」をオンにし、エンジンの URL を入力します(例 `http://127.0.0.1:50021`)。
3. 「接続テスト」を押すと、選べる声の一覧が表示されます。既定の声を選んで「保存」します。「試し聞き」で確認できます。

### カラムごとの設定

読み上げたいカラムをダブルクリックして編集し、「このタブの読み上げ」をオンにします(エキスパートモードでは、投稿者名を読むか・待ち件数の上限・このカラムの声も選べます)。

- 読み上げるのは、そのカラムに**新しく届いた**ノートです。起動時や再接続時に取得した過去のノート、自分の投稿、通知カラムは読み上げません。
- CW 付きのノートは CW だけを読み、本文は読みません。純粋なリノートは読みません。URL は「URL」と読み、絵文字やコードの中身は読みません。
- 読み上げが追いつかないときは、待ち件数の上限(既定 5)を超えた分が、古いものから捨てられます。

### 注意

- VOICEVOX 系の声を使った音声を公開する場合は、「VOICEVOX:キャラクター名」のクレジット表記と、各キャラクターの利用規約の遵守が必要です。
- エンジンの URL は、設定のエクスポート・インポートの対象外です(インポートしても書き換わりません)。
```

(Task 1 の結論が「Android の LAN の http に接続できない」だった場合は、「注意」に「Android では `https://` の URL が必要な場合があります」を足す。)

- [ ] **Step 2: CLAUDE.md の構成の説明を足す**

`CLAUDE.md` の `### src-tauri/src layout` の `translate/` の項目の後に、次の項目を足す(既存の項目と同じく1つの箇条書き)。

```markdown
- `tts/` — カラム単位のノート読み上げ(Issue #625、設計は `docs/superpowers/specs/2026-11-19-issue-625-column-speech-design.md`)。フロント(`store.svelte.ts` の `columnNote` リスナーと `lib/speechText.ts`)が発火判定と `mfm-js` による整形を行い、整形済みの文を `speak_note` で渡す。Rust 側は `engine.rs`(`SpeechEngine` trait)・`external.rs`(外部の VOICEVOX 互換エンジン = VOICEVOX ENGINE / AivisSpeech Engine の `/speakers`・`/audio_query`・`/synthesis`。専用の `reqwest::Client` で、タイムアウトあり・リダイレクトなし・接続先は設定済みの URL のみで、`net_guard` は使わない)・`queue.rs`(全体で1本の FIFO、上限はカラムごと、同じノート ID は積まない)・`manager.rs`(`SpeechManager` と、「取り出す → 合成 → 再生 → 次へ」のループ。連続3回失敗で `SpeechStatus::Error` を1回だけ通知してキューを捨てる)・`runtime.rs`(`AppHandle` 経由の配線)。再生は `sound.rs` の `SoundPlayer` の**読み上げレーン**(`speech_lane.rs`、rodio の `Player` で逐次再生・停止・完了通知。通知音の `play` とは Mixer 上でミックス)。設定は `Column.speech`(`#[serde(default)]`、エクスポートに含む)と `SettingsData.speech`(`external_url` を含み、**エクスポートに含めず、インポートで現在の値を維持**)。段階2で内蔵 VOICEVOX CORE を同じ trait で足す予定(別 spec)。
```

- [ ] **Step 3: Rust とフロントの全体テスト**

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: PASS(`#[ignore]` を除く)。Run: `cd frontend && pnpm check && pnpm test 2>&1 | tail -10`
Expected: PASS。

- [ ] **Step 4: 実エンジンでの結合確認(Xvfb 越し)**

CLAUDE.md と過去の運用に従う(`WAYLAND_DISPLAY` を unset、`dbus-run-session`、ユーザーの実画面に出さない)。ここでは音そのものは聞けないので、**キュー・合成・状態遷移をログと debug bridge で確認する**。

1. VOICEVOX ENGINE を起動する: `docker run -d --name tsumugi-vv-e2e -p '127.0.0.1:50021:50021' voicevox/voicevox_engine:cpu-latest`。
2. 隔離した tsumugi(`reference-isolated-instance-needs-short-xdg-path` に従い `/tmp` 直下の短い XDG パス)を Xvfb 上で `cargo tauri dev` で起動する(リポジトリのルートから)。
3. debug bridge(`curl --unix-socket <path> http://localhost/ --data-binary '<js>'`、JS は同期関数、`return` が必要)で、次を確認する。
   - `listSpeechVoices("http://127.0.0.1:50021")` が声の一覧を返す(ブリッジは await を扱えないので、kick-and-poll で結果を `window` に置いて読む)。
   - `setSpeechSettings` で `enabled: true`・URL・声を保存し、`speakNote` を連続で6件呼ぶ。Rust 側のログ(`enable_file_logging` をオンにして `log` を確認)で、合成が順番に行われ、上限を超えた分が捨てられていること。
   - エンジンを止める(`docker stop`)と、3件の連続失敗の後に `SpeechStatusChanged`(`error`)が1回だけ届くこと。再起動すると次の読み上げで `ok` に戻ること。
4. 結果を `2026-11-19-issue-625-spike-notes.md` の末尾に「結合確認」として追記する(確認した項目と、確認できなかった項目)。
5. 片付ける: 自分で起動した `cargo tauri dev` と Xvfb を PID 指定で kill し、`docker rm -fv tsumugi-vv-e2e` する。

実際に音が鳴ること(スピーカーからの再生)は、この環境では確認できない。**ユーザーの環境で聞いて確認してもらう**項目として、最終報告に明記する。

- [ ] **Step 5: スパイク(c): Android での読み上げレーン**

`reference-android-emulator-environment` の手順に従い、エミュレーターで確認する(`AVD` はコピーして使う、`ANDROID_NDK_ROOT=<r27c>` と `ANDROID_PLATFORM=android-26` を必ず export する、CLAUDE.md の Android の節)。

1. まず、ビルドが通ること: `cd src-tauri && cargo tauri android build --debug --target x86_64`(エミュレーター向け)。`tts/` と `speech_lane.rs` は純 Rust なので、通るはず。
2. エミュレーターで起動し、設定 → 読み上げ の画面が出ること、エンジン URL に `http://10.0.2.2:50021`(エミュレーターから見たホスト)を入れて「接続テスト」の結果を見る。**これで Task 1 の(b)(平文 HTTP)の結論も実機で裏づく。**
3. 「試し聞き」で、`SoundPlayer` の読み上げレーンがエラーなく再生を開始すること(エミュレーターの音声出力はログで確認する)。

確認できなかった場合(エミュレーターが使えない等)は、「Android の実機確認は未実施」と記録して最終報告に書く(「adb devices が空」で未検証にせず、SDK・AVD を探して実際に試すこと)。

`2026-11-19-issue-625-spike-notes.md` の「(c)」の節を、実際の結果で置き換える。

- [ ] **Step 6: Commit**

```bash
git add docs CLAUDE.md
git commit -m "docs(tts): 読み上げのユーザーガイドと構成の説明を追加(Issue #625)"
```

- [ ] **Step 7: ブランチ全体のレビューと PR**

`superpowers:finishing-a-development-branch` に従って、ブランチ全体をレビューし、PR を作る。PR 本文は `.github/pull_request_template.md` の構造に手で合わせ(`--body` を渡すとテンプレートが使われない)、`Closes #625` ではなく、**段階1だけなので `Refs #625`**(段階2・3が残っているため、Issue を自動で閉じない)にする。マージは `gh pr merge --merge`(squash しない)。push 後に CI を待たない(ユーザーが見る)。

---

## Self-Review(計画の自己点検)

**1. Spec coverage**

| spec の項目 | 担当 |
|---|---|
| §1 `ColumnSpeech` / `SpeechSettings`、JSON への保存、エクスポート・インポート | Task 2 |
| §2 エンジン trait・`ExternalEngine`(タイムアウト・リダイレクトなし・`net_guard` 不使用・識別コード) | Task 3 |
| §3 `SoundPlayer` の読み上げレーン | Task 5 |
| §3 キュー(カラムごとの上限・ID 重複排除・カラム削除) | Task 4 |
| §3 ワーカー(連続失敗・状態通知・`stop_speech`・カラムオフ) | Task 6、Task 7 |
| §4 発火(`columnNote`、`speak:` 枠、ギャップ埋め除外、自分のノート除外)と整形 | Task 8、Task 9 |
| §4 `speak_note` の Rust 側再検査(`enabled`・文字数) | Task 7 |
| §5 設定画面・カラム設定・コマンド/イベント登録・エラー表示・ガイド | Task 7、Task 10、Task 11 |
| テスト方針(Rust 単体・フェイクサーバー・manager・フロント・バインディング・実環境) | 各タスク、Task 11 |
| 段階1のスパイク (a)(b)(c) | Task 1、Task 11 |
| 非目標(通知カラム・ギャップ埋め・先読み・Android のオーディオフォーカス) | Global Constraints、Task 9、Task 10 |

spec の「フロントは `commands.speakNote({ columnId, noteId, text })`」は、tauri-specta の生成が**位置引数**(`commands.speakNote(columnId, noteId, text)`)であることに合わせて、本計画では位置引数で書いている(挙動の差はない)。

**2. Placeholder scan**: `TBD` / `TODO` / 「適切にエラー処理する」/ 「同様に」の類は無い。Android の注意書きは、Task 1 の結論に依らない書き方にして、プレースホルダを残していない。

**3. Type consistency**: `ColumnSpeech`(`enabled` / `speak_author` / `queue_limit` / `voice_id`)は Task 2 → 7 → 8 → 9 → 10 で同名(TS では camelCase)。`SpeechSettings` のフィールド(`enabled` / `engine` / `external_url` / `default_voice_id` / `speed_scale` / `volume`)も同じ。`SpeechItem` / `SynthParams` / `PushOutcome` は Task 3・4 の定義と Task 6・7 の使用が一致。`SpeakOutcome`(`Finished` / `Stopped` / `Failed`)は Task 5 の定義と Task 6 の使用が一致。`WorkerContext` の4メソッドは Task 6 の定義、Task 7 の `AppContext`、Task 6 のテストの偽実装で一致。`TEST_COLUMN_ID` / `SPEECH_TEXT_MAX_CHARS` は Task 7 内で定義と使用が一致。`pop_for_test`(Task 6、同モジュール内)と `pop_for_command_test`(Task 7、他モジュールのテスト用)は別名で、混同しない。

**4. Review Focus**: 5項目すべてに対応するテストがある(1: Task 3 の `a_hanging_engine_times_out…`・Task 6 の失敗のテスト、2: Task 8 の巨大本文・ネスト・Task 7 の 300 文字、3: Task 8 の「中身がないノート」、4: Task 4 の重複排除・Task 7 の複数カラム・Task 9 の `speak:` 枠、5: Task 6 の `stopping_while_synthesizing…`・Task 7 の `turning_the_master_switch_off…`・`closing_a_column…`)。

**既知のリスク(実行時に確認する点)**
- Task 5 の `poll_reports_finished_once_the_buffer_has_been_played` は、rodio の内部(`Done` ラッパーが `sound_count` を減らすタイミング)に依存する。失敗したら、`drain` のサンプル数ではなく、`empty()` が真になる条件を `rodio-0.22.2/src/player.rs` で確認する。
- Task 7 Step 4 の `close_column` の編集は、その関数の既存の変数名に合わせる必要がある(計画では場所の探し方だけを示している)。
- Task 8 の MFM の期待値(特に `$[tada …]` と数式)は、`mfm-js` 0.26 の実際の AST で確認する。
