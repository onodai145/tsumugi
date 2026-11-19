# カラム読み上げ設計 (Issue #625)

## 背景と目的

特定のカラム(タブ)に届いた新着ノートを、音声合成で自動的に読み上げる。Issue 本文は「VOICEVOXを使う？それとも内蔵する？」の一行のみで、以下はブレインストーミングで合意した内容である。

- 読み上げの品質を重視する。VOICEVOX 系の自然な声を使いたい。
- 外部のエンジンに接続する方式と、アプリ内蔵の方式の両方を、段階的に提供する。
- 対象は Android を含む。ただし Android の内蔵エンジンは実機での実測(スパイク)の結果で判断する。

## 決定事項(合意済み)

- 段階分け: 段階1 = エンジン抽象 + 外部エンジン(VOICEVOX 互換 API)+ 読み上げ機能一式(全プラットフォーム)。段階2 = デスクトップ向けの内蔵 VOICEVOX CORE。段階3 = Android の内蔵(スパイク先行)。この spec は段階1を詳細に定め、段階2・3は境界(trait との接点)だけを定める。段階2・3は別の spec・plan を書く。
- 溢れたときの扱い: 上限付きキュー。上限はカラムごとの設定(既定 5、範囲 1〜20)で、超えたらそのカラムの最古の待ち項目を捨てる。
- 読み上げる内容: CW があれば CW だけ、なければ本文。投稿者名の前置き(`{表示名}さん、`)は既定でオン(カラムごとに切り替え可)。純粋なリノートと、整形後に空になるノートは読まない。
- アーキテクチャ: ハイブリッド。フロントが発火判定と整形(`mfm-js`)を行い、Rust が合成・キュー・再生を持つ。発火元だけを Rust に移す形への発展は、あとから可能な構造にしておく。

## 目標 / 非目標

目標:

- カラムごとに読み上げのオン・オフを設定でき、オンのカラムに届いたライブのノートが順番に読み上げられる。
- 外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE、AivisSpeech Engine)に接続して動く。
- 既定はオフで、既存の設定・保存データがそのままで従来どおり動く。
- 読み上げの失敗(エンジン停止など)が、ノートの受信・表示・通知を妨げない。

非目標(段階1):

- 内蔵エンジン(段階2・3)。
- 通知カラム(メンション・リアクションの通知)の読み上げ。ノートのタイムラインだけを対象にする。
- 起動時・再接続時のギャップ埋めで取得したノートの読み上げ(通知音と同じ理由で、不在中に溜まったノートでの誤爆を避ける)。
- 先読み合成、声ごとのカラム別の速度・音量、ユーザー辞書の編集。
- Android のオーディオフォーカス(他アプリの音声との競合)の制御。

## 設計

### 1. データモデルと設定

カラム単位の設定 `Column.speech: ColumnSpeech`(`domain/column.rs`、`specta::Type`、`camelCase`):

| フィールド | 型 | 既定 | 意味 |
|---|---|---|---|
| `enabled` | bool | false | このカラムを読み上げる |
| `speak_author` | bool | true | 投稿者名を前置きする |
| `queue_limit` | u8 | 5 | このカラムの待ち件数の上限(1〜20 に丸める) |
| `voice_id` | String | "" | 空ならグローバルの既定の声を継承する |

永続化は `column_def` に `speech_json TEXT NOT NULL DEFAULT ''` を1列だけ追加する(`kind_json` / `filter_json` の前例に倣う)。空文字は既定値として扱う。マイグレーションは冪等にし、「列が無ければ `ALTER TABLE`」で行う(`store/db.rs` の既存の流儀)。`Column` に項目が増えるため、リテラルで構築している箇所(`attribution.rs`、`commands/column.rs`、`commands/settings_transfer.rs`、`commands/mute.rs`、`store/settings.rs`、`store/settings_transfer.rs` のテストなど)を更新し、TS バインディングを再生成する。

グローバル設定 `SettingsData.speech: SpeechSettings`:

| フィールド | 型 | 既定 | 意味 |
|---|---|---|---|
| `enabled` | bool | false | 全体のマスタースイッチ |
| `engine` | enum | `external` | 段階1は `external` のみ。段階2で `voicevox_core` を追加する |
| `external_url` | String | "" | 外部エンジンの URL(例 `http://192.168.1.10:50021`) |
| `default_voice_id` | String | "" | 既定の声 |
| `speed_scale` | f32 | 1.0 | 話速 |
| `volume` | f32 | 1.0 | 音量 |

エクスポート・インポート(Issue #544): カラムの読み上げ設定(`Column.speech`)は含める。`SettingsData.speech`(特に `external_url`)は含めず、インポートでは現在の値を維持する(MCP 設定と同じ扱い。他人の設定ファイルを読み込むだけで、任意のホストへ接続しに行く状態を避ける)。

### 2. エンジン抽象と外部エンジン

`src-tauri/src/tts/` を新設する(`translate/` に倣い、Tauri への依存はコマンド層に閉じる)。

`tts/engine.rs`:

```rust
trait SpeechEngine: Send + Sync {
    async fn voices(&self) -> Result<Vec<Voice>>;
    async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>>; // WAV
}
struct Voice { id: String, label: String }   // id はエンジン固有の不透明な文字列
struct SynthParams { speed_scale: f32, volume: f32 }
```

`id` を文字列にするのは、段階2の CORE(スタイル ID)や将来のエンジンでも同じ型で扱えるようにするため。

`tts/external.rs`(VOICEVOX 互換クライアント)。公式 README で確認した API は、`/speakers`(声の一覧。`id` がスタイル ID)、`/audio_query?speaker=ID&text=…`(POST。読み・イントネーションのクエリ)、`/synthesis?speaker=ID`(POST。クエリを渡すと WAV)。AivisSpeech Engine は VOICEVOX ENGINE ベースで API 互換を保つ方針と README に明記されている(ただし1人用途向けで、大量リクエストの処理には最適化されていない)。

- `/audio_query` の応答は `serde_json::Value` のまま扱い、`speedScale` と `volumeScale` だけを上書きして `/synthesis` に渡す(エンジン間の差異やフィールド追加に強くするため)。
- 専用の `reqwest::Client` を使う。接続タイムアウトと全体タイムアウトを必ず設定し(reqwest の既定は無制限)、リダイレクトは追わず、設定済みの URL にだけ接続する。URL は `http` / `https` だけを許可する。
- `net_guard` の SSRF ガードは使わない。このガードは WebView が渡す任意の URL 向けの仕組みで、読み上げの URL はユーザーが設定画面で保存した値だけである(インポートでは引き継がない)。ループバック・LAN を許可するのは、HomeLab での利用が前提だから。
- エラーは `Error::Invalid` の識別コード(`SPEECH_ENGINE_UNREACHABLE` など。翻訳の `LOCAL_*` と同じ流儀)で返す。

### 3. キューと再生

#### 再生: `SoundPlayer` に読み上げレーンを追加する

現在の `SoundPlayer`(`sound.rs`)は投げっぱなしで、完了通知も停止もなく、重なった音はミックスされる。読み上げは「順番に1つずつ」「完了を待つ」「止められる」ことが必要なので、rodio 0.22 の `Player`(`connect_new` / `append` / `stop` / `empty` / `set_volume`)で、同じ Mixer 上に専用のレーンを作る。

- 再生スレッドのチャンネルを `Vec<u8>` から enum(`Play(bytes)` | `Speak { wav, done }` | `StopSpeech`)に変える。`play()` の挙動と呼び出し側は変えない。
- 読み上げレーンは専用の `Player` で逐次再生し、再生スレッドが `empty()` を見て完了を `oneshot` で通知する。出力デバイスを開き直したときは `Player` も作り直す。
- 通知音との重なりは、これまでどおり Mixer でミックスされる。
- `sound.rs` は Issue #12(原因の診断が困難だった障害)の経緯を持つ慎重な領域である。変更は読み上げレーンの追加に限り、既存の `play` 系のテストが通ることを必須とする。

#### キュー(`tts/queue.rs`): 音声と切り離した純粋な状態機械

- 全体で1本の FIFO(声が同時に重ならないようにする)。
- 上限はカラムごと。あるカラムの待ち件数が `queue_limit` を超えたら、そのカラムの最古の項目を捨てる。
- 同じノート ID は二重に積まない(複数カラムに同じノートが来る場合の安全策)。
- 先読み合成はしない。

#### ワーカー(`tts/manager.rs`): tokio の常駐タスク

- 「取り出す → 合成 → レーンに送る → 完了待ち → 次へ」を繰り返す。
- 合成に失敗した項目は、ログに残してスキップする。
- 連続して失敗したら、`SpeechStatusChanged` イベントを状態が変わったときだけ1回送り、キューを空にする。次の項目が来たら再試行する。
- `stop_speech` はキューを空にして再生中の読み上げを止める。カラムの読み上げをオフにしたときは、そのカラムの待ち項目だけを消す。

### 4. フロントの発火と整形

発火点は `events.columnNote` のリスナー(`store.svelte.ts`)の、通知音の判定の直後。ライブで届いたノートだけが対象で、`columnGapFill` では読まない。

条件: 自分のノートではない、グローバルの `speech.enabled`、このカラムの `speech.enabled`、重複排除の通過。重複排除は `#markNotified` を使うが、キーは `speak:<noteId>` にして、通知音・デスクトップ通知とは別枠にする(通知音が枠を消費して読み上げが落ちるのを防ぐ)。

`lib/speechText.ts`(新規の純粋関数): `buildSpeechText(note, { speakAuthor }) → string | null`。`null` は「読まない」。

- 純粋なリノート(本文なし)は `null`。引用リノートはコメント部分だけを読む。
- CW があれば CW の文字列だけを読み、本文は読まない。
- 本文は `mfm-js` の `parse` で AST にして変換する。テキストは残す。装飾(太字、`$[fn …]` など)は子要素をそのまま読む。メンションはユーザー名だけ(`@user@host` は `user`)。ハッシュタグは `#` を除いた語。URL は「URL」と読み、ラベル付きリンクはラベルを読む。コードは内容を読まず「コード」と読む。カスタム絵文字、Unicode 絵文字、数式、検索ボックスは省略する。
- 空白と改行を整える(改行は読点に)。長さが上限(120 文字)を超えたら、切って「以下略」を付ける。整形後に空なら `null`(画像だけのノートなど)。
- `speakAuthor` がオンなら `{表示名}さん、` を前置きする。表示名はカスタム絵文字を除き、空なら `username` にする。

IPC: `commands.speakNote({ columnId, noteId, text })`。ラッパー(`ipc.ts`)は `playNotifySound` と同じく例外を投げず、失敗はデバッグログだけに残す。声・速度・キュー上限は Rust が設定から引く(設定の取得元を1つにするため)。Rust 側でもカラムの `enabled` と文字数の上限を再検査する(設定を切り替えた直後の競合と、不正な入力への備え)。

### 5. UI

全体設定に新セクション「読み上げ」(`ui/settings/SpeechSection.svelte`)を追加する。

- マスタースイッチ。
- エンジンの URL 入力(段階1は外部エンジンのみ。選択肢は段階2で内蔵が加わるときに出す)。
- 「接続テスト」ボタン。成功したら声の一覧を取得して、既定の声のセレクトに反映する。
- 速度と音量。
- 「試し聞き」(固定の文を合成して再生する `test_speech`)と「停止」。
- 利用する声のキャラクターの規約に従う旨の注意書き。VOICEVOX 系の規約ではクレジット表記が必要(voicevox_vvm の TERMS)。法的な判断はせず、注意書きにとどめる。
- 保存は他のセクションと同じ `SaveBar` の流れにする。

カラム設定(`AddColumnModal.svelte` の通知設定の隣):

- 「読み上げ」トグルは常に表示する。
- 詳細(投稿者名を読む、待ち上限、声)は expert モードのときだけ表示する。guided では既定値で動く(通知音の `soundPickerShown` と同じ考え方)。
- グローバルがオフのときはトグルを無効にし、「設定で読み上げをオンにしてください」と案内する。

コマンドとイベント(`lib.rs` の `specta_builder()` に登録する): `get_speech_settings` / `set_speech_settings` / `set_column_speech` / `list_speech_voices` / `test_speech` / `speak_note` / `stop_speech`、Event `SpeechStatusChanged`(接続中・到達不能など。状態が変わったときだけ送る)。

エラー表示は、設定画面のステータス表示と Backstage ログに出す。カラムにバッジなどは出さない。`set_speech_settings` は、URL のスキームが http / https でないもの、上限値の範囲外を弾く。

`docs/guide/user-guide.md` に読み上げの節を追記する。新しい UI の値は `docs/design/style-guide.md` の既存の目盛りだけを使う。

## テスト

- Rust の単体テスト: `queue.rs`(上限による破棄、ノート ID の重複排除、カラム単位の削除)、設定のバリデーション、`ColumnSpeech` の serde 既定値(`speech_json` が空でも既定値になること)、`store/db.rs` のマイグレーションの冪等性。
- 外部エンジンのクライアント: テスト内のフェイク HTTP サーバーで、`/speakers`・`/audio_query`・`/synthesis` の流れ、タイムアウト、到達不能を検証する(既存の `axum` を流用できるかは plan で確認する)。
- `manager.rs`: フェイクのエンジンとフェイクの再生レーンを差し込み、順序、失敗したときのスキップ、連続失敗でイベントが1回だけ出ることを検証する。
- `SoundPlayer` の読み上げレーン: デバイスなしで確認できる部分だけを単体テストにする。既存の `sound.rs` のテストが通ることを必須条件にする。
- フロント(Vitest): `speechText.test.ts`(MFM の種類ごとの表形式)、`AddColumnModal` のトグルの出し分け、`SpeechSection` の接続テストと保存。
- バインディング: 既存の `generates_frontend_bindings` テストで、新しいコマンドとイベントを検証する。
- 実環境での確認: VOICEVOX ENGINE を Docker で起動し、Xvfb 越しにキュー・合成・状態遷移をログで確認する。実際に音が鳴るかは、ユーザーの環境で手動確認する(レビューが通っただけで動作確認の代わりにはしない)。

## 段階分け

- 段階1(この spec): 上記の設計すべて。plan の最初に3つのスパイクを置く。(a) AivisSpeech で `speedScale` / `volumeScale` が効くか。(b) Android で LAN 内の平文 HTTP が許可されるか。(c) rodio の `Player` による読み上げレーンが Android でも動くか。
- 段階2(別 spec・別 plan): デスクトップ向けの内蔵 VOICEVOX CORE。確認済みの前提は次のとおり。CORE 本体は MIT(0.16 以降)。Rust クレート `voicevox_core` は `publish = false` で crates.io に無く、git 依存になる(`llama-cpp-2` と同じ扱い)。MSRV は `rust-version = "1.89.0"` で、本リポジトリの 1.88 を引き上げる必要がある。動作には ONNX Runtime(動的ロード)、OpenJTalk 辞書、VVM(音声モデル、1個約 55〜60 MB)が別に必要で、同梱せず初回に取得する(`translate/download.rs` の Range 再開・SHA-256 検証を流用)。VVM の規約(組み込み再配布可、VOICEVOX を使った旨のクレジット表記が必要、キャラクターごとの規約に従う)への同意を Rust 側で強制する(翻訳のライセンス同意と同じ)。この段階でエンジン設定に `voicevox_core` を追加する。
- 段階3(別 spec・別 plan): Android の内蔵。Android 向けの CORE ビルドは arm64 と x86_64 だけ(armv7 と i686 には無い)で、ONNX Runtime は別途取得する。16KB ページへの対応には VOICEVOX ONNX Runtime 1.22 以降が必要。先に実機で速度・メモリ・16KB ページを実測するスパイクを行い、実用的なら進める。armv7 と i686 では内蔵エンジンを無効にする(CI は4つの ABI をビルドする)。

## リスクと未確認事項

- AivisSpeech の API 互換の範囲(`speedScale` / `volumeScale` の効き方など)。段階1のスパイク(a)で確認する。
- Android で LAN 内の平文 HTTP を許可する設定(`cleartextTraffic` / `networkSecurityConfig`)の要否。スパイク(b)で確認する。
- Android のバックグラウンドで WebView が止まったときの挙動。通知音と同じ経路なので同じ制約を受ける可能性がある(未検証)。必要なら、発火元を Rust に移す形(アプローチ B)へ発展させる。
- キャラクターごとの利用規約の違い(個人と法人で条件が分かれるキャラがいる)。段階2で内蔵する声を選ぶ際に個別に確認する。
- 日本語以外のノート(英語のユーザー名や本文)の読みはエンジン次第で、品質は保証しない。
