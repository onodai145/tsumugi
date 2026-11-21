# 内蔵 VOICEVOX CORE(段階2)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** デスクトップ(Linux・macOS・Windows)で、外部エンジンなしで読み上げられるよう、公式の VOICEVOX CORE(C API)を実行時に取得して動的ロードする内蔵エンジンを `SpeechEngine` trait に足す。

**Architecture:** `src-tauri/src/tts/voicevox/` に、固定値のカタログ・ディスク配置・安全なアーカイブ展開・C API の FFI ラッパ・`CoreEngine: SpeechEngine`・取得/削除の状態機械(`Packs`)を置く。`translate/download.rs` の `download()` を再利用する。段階1のキュー・ワーカー・再生・フロントの発火判定は変えず、`AppContext::engine()` が設定の `engine` で `ExternalEngine` と `CoreEngine` を切り替える。UI は設定 → 読み上げに、エンジンの切替・エンジン一式の取得・声(VVM)の管理を足す。

**Tech Stack:** Rust(tokio, async-trait, reqwest, wiremock, `libloading` 0.8, `zip` 2, `tar` 0.4, `flate2` 1)、tauri-specta、Svelte 5 + Vitest。

**Spec:** `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`(段階1: `docs/superpowers/specs/2026-11-19-issue-625-column-speech-design.md`)

## plan 作成時に実測して分かったこと(spec への訂正は Task 1 で反映する)

実物(上流のリリース)を取得して確認した。スクラッチの確認結果は Task 1 でスパイクノートに残す。

- **VVM は 27 個、約 1.65 GB**(spec の「28 個・約 1.7 GB」を訂正)。`s0.vvm`(130 MB)は歌声用のスタイル(`frame_decode` / `sing`)だけで、読み上げには使えない。読み上げに使える種別は `streaming_talk`(137 スタイル、ID の重複なし)。
- **ONNX Runtime 1.17.3 で動く**。CORE 0.17.1 の「推奨」は 1.23.2(prerelease)だが、`voicevox_onnxruntime_load_once` にファイルパスを明示すれば 1.17.3 でロードできる。Linux x64 で、24 kHz・モノラル・16 bit の WAV を合成できた(最大振幅 9310、ほぼ全区間が無音ではない)。
- **ONNX Runtime の規約**は、リリース本文だけでなく、tgz 内の `TERMS.txt`(938 バイト)にある。Windows 版は 960 バイト(改行コードの違い)。同意版は改行コードを除いてハッシュする。
- **クレジット表記は規約から取り出す**(キャラごとに「VOICEVOX:ずんだもん」「VOICEVOX:Voidoll(CV:丹下桜)」など表記が違う。`n0` は「VOICEVOX Nemo」)。規約に個別の節がない `雨晴はう` は、全体の規約に従い `VOICEVOX:雨晴はう` にする。
- **Linux のシステムライブラリ**は、`libstdc++`・`libgcc_s`・`libm`・`libc` だけ。`libgomp` は不要。
- 実測(16 コア、推論 8 スレッド): ONNX Runtime のロード 0.01 秒、VVM(約 59 MB)のロード 0.6 秒、5.1 秒ぶんの音声の合成が約 1.0 秒(2 回目 0.84 秒)、最大メモリ 278 MB。
- **VVM のハンドルは、ロード直後に閉じてよい**(`voicevox_voice_model_file_delete` のあとでも合成できる)。
- 取得物の SHA-256 は、`voicevox_core` と `voicevox_vvm` は GitHub の asset のダイジェスト、ONNX Runtime と辞書は実物から計算した(Task 2 の表)。
- spec との差(Task 1 で spec を直す): (1) 声の一覧は、既存の `list_speech_voices` の分岐ではなく、新しいコマンド `list_speech_core_voices` にする(既存は URL を引数に取るため)。(2) `cfg(desktop)` で囲むのは、ネイティブの呼び出し(`ffi.rs` の `NativeCore`)だけにする。コマンドとイベントはプラットフォーム非依存でないと、TS バインディングが環境ごとに変わってしまうため。ほかのモジュールは全環境でビルドし、Android では `supported = false` を返す。

## Global Constraints

- CORE は 0.17.1、ONNX Runtime は 1.17.3(正式リリースの最新。1.23.2 は prerelease)、辞書は `r9y9/open_jtalk` v1.11.1 の `open_jtalk_dic_utf_8-1.11.tar.gz`、VVM は `voicevox_vvm` 0.17.1 の読み上げ用(`talk` / `streaming_talk`)スタイルを持つ 27 個。
- 対応する環境は Linux(x86_64・aarch64)、macOS(x86_64・aarch64)、Windows(x86_64)。それ以外は `supported = false`。Android・Windows arm64・BSD は内蔵を選べない。
- Rust クレート `voicevox_core` の git 依存、`ort`、`open_jtalk-rs`(C++)は**足さない**(ビルドを重くしない)。追加する依存は `libloading`・`zip`・`tar`・`flate2` だけ。
- **`unsafe` は `ffi.rs` に閉じ込める**。`NativeCore` は `#[cfg(desktop)]`。
- 取得物は SHA-256 で検証する(`translate/download.rs` の `download()` を使う)。ロード前にファイルサイズを固定値と照合する。ロードのたびの再ハッシュはしない。
- **規約への同意は Rust 側で強制する**。同意版は、埋め込んだ 2 つの規約の写しの SHA-256(改行コードを除く)。`start_speech_core_download` の引数の版が一致しなければ拒否する。同意版は取得の開始に成功したあとに保存する。`SettingsData.speech_core_terms_version` は、設定のエクスポートに含めず、インポートでは現在の値を維持する。
- 既定のエンジンは `external` のまま。外部エンジンと内蔵で `voice_id`(スタイル ID の数値の文字列)は共通。
- 推論のスレッド数はコア数の半分(最低 1)。アイドル解放は 10 分(`IDLE_UNLOAD`)、掃除の間隔は 30 秒。
- 利用者向けの文面にパス・URL を含めない。ログには失敗の種類だけを出す。
- エラーコード: `ERR_NOT_READY = "SPEECH_CORE_NOT_READY"`、`ERR_ENGINE_LOAD = "SPEECH_CORE_LOAD_FAILED"`、`ERR_UNSUPPORTED = "SPEECH_CORE_UNSUPPORTED"`(`tts/mod.rs`)。
- 取得物の置き場: `app_data_dir/voicevox/`(`engine-<ENGINE_SET>/`、`vvm-<VVM のリリース>/`、`dl/`)。
- フロントは、`#[serde(default)]` で省略可能になる TS の型を、`lib/speechDefaults.ts` で補ってから使う。生成された `commands.*` は `invoke` の生の戻り値を `{status:"ok", data}` に包む(テストでは `vi.spyOn(commands, "…").mockResolvedValue(ok(...))`)。
- ブランチ: `feat/issue-625-voicevox-core-stage2`(作成済み)。コミットは件名だけ・Conventional Commits・末尾に `Co-Authored-By` のトレーラー(`-m` を 2 つ)。**コミットが失敗・タイムアウトしたら止めて報告する**(再試行しない)。`--no-verify` / `--no-gpg-sign` は使わない。
- push 後に CI を監視しない。
- Rust のテストは `cd src-tauri && cargo test --lib tts::voicevox`(フィルタは `tts::` で始める。`speech` だと `tts::external` が外れる)。フロントは `cd frontend && npx vitest run <path>`、型は `pnpm check`。
- 大きなファイルの置き場に `/tmp`(tmpfs = RAM)を使わない。実環境の確認用の取得物は `$HOME/.cache/tsumugi-voicevox-probe/` に置く。

## Review Focus

spec が暗に求めているが、どのタスクのテストも正面からは見ていない、実際に起きやすい入力・状況。各行のテストは、担当のタスクに足してある。

1. **同意版が空・古い・別物**: 取得は拒否され、同意版は保存されない(`start_speech_core_download`)。→ Task 9
2. **悪意あるアーカイブ**: `..` や絶対パスのエントリ、シンボリックリンク、展開後に巨大になるファイル。→ Task 4(エンジン一式の取得に失敗したときの後始末は Task 7)
3. **取得の途中で通信が切れる・キャンセル・ハッシュ不一致**: `.part` は再開でき、ハッシュ不一致では捨てる。失敗したエンジン一式の途中のディレクトリが残らない。→ Task 7
4. **内蔵を選んでいるのに、エンジン一式か声が未取得・声の ID が数値でない・カタログにない・空**: ワーカーは落ちず、`ERR_NOT_READY` / `ERR_NO_VOICE` になり、連続 3 回で `Error` 状態になる。ネイティブコードが panic しても、`synthesize` は `Err` を返す。→ Task 6
5. **Android や対応外の環境に、`engine = voicevoxCore` の設定が入る**(設定のインポートや、別マシンからの設定): 起動も読み上げも落ちず、`ERR_UNSUPPORTED` が状態に出る。→ Task 6・Task 9

## ファイル構成

作成(Rust、`src-tauri/src/tts/voicevox/`):
- `mod.rs` — モジュール宣言、`SpeechCore`(`store` + `engine` + `packs` をまとめる)、`native_factory`
- `catalog.rs` / `catalog.json` / `terms/voice-models.txt` / `terms/onnxruntime.txt` — 固定値
- `store.rs` — ディスク配置・状態の検査・掃除
- `archive.rs` — zip / tgz の安全な展開
- `ffi.rs` — `CoreBackend` trait、`NativeCore`(`cfg(desktop)`)
- `engine.rs` — `CoreEngine: SpeechEngine`、アイドル解放
- `packs.rs` — 取得・削除・進捗の状態機械、フロント向けの `SpeechCoreInfo`
- `test_util.rs`(`cfg(test)`) — テスト用のアーカイブ作成

作成(その他):
- `src-tauri/src/commands/speech_core.rs` — コマンド
- `scripts/gen-voicevox-catalog.py` — カタログの生成
- `frontend/src/lib/speechCore.ts` + `speechCore.test.ts` — 文面・クレジットの引き当て
- `frontend/src/ui/settings/SpeechCoreTermsDialog.svelte` + `.test.ts` — 規約の確認
- `frontend/src/ui/settings/SpeechCoreSection.svelte` + `.test.ts` — エンジン一式・声の管理
- `docs/superpowers/plans/2026-11-20-issue-625-stage2-spike-notes.md`

変更: `src-tauri/Cargo.toml`、`tts/mod.rs`、`tts/runtime.rs`、`domain/speech.rs`、`store/settings.rs`、`store/settings_transfer.rs`、`state.rs`、`events.rs`、`commands/mod.rs`、`lib.rs`、`frontend/src/bindings/tauri.gen.ts`(生成)、`ui/settings/SpeechSection.svelte`(+ test)、`ui/AddColumnModal.svelte`、`docs/guide/user-guide.md`、`CLAUDE.md`、spec。

---

### Task 1: カタログの生成・規約の写し・spec の訂正・スパイクノート

**Files:**
- Create: `scripts/gen-voicevox-catalog.py`
- Create: `src-tauri/src/tts/voicevox/catalog.json`(生成物)
- Create: `src-tauri/src/tts/voicevox/terms/voice-models.txt`、`src-tauri/src/tts/voicevox/terms/onnxruntime.txt`
- Create: `docs/superpowers/plans/2026-11-20-issue-625-stage2-spike-notes.md`
- Modify: `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`

**Interfaces:**
- Produces: `catalog.json` の形(Task 2 が読む): `{"vvm_release": "0.17.1", "vvms": [{"id": "0", "size": 59308488, "sha256": "…", "characters": [{"name": "ずんだもん", "credit": "VOICEVOX:ずんだもん", "styles": [{"id": 3, "name": "ノーマル"}]}]}]}`

- [ ] **Step 1: 生成スクリプトを書く**

`scripts/gen-voicevox-catalog.py`(VVM 本体は取得しない。zip の末尾だけを Range 取得して `metas.json` を読む):

```python
#!/usr/bin/env python3
"""VOICEVOX の VVM カタログ(`catalog.json`)を、上流のリリースから生成する(Issue #625 段階2)。

使い方: python3 -I scripts/gen-voicevox-catalog.py > src-tauri/src/tts/voicevox/catalog.json
VVM 本体は取得しない。zip の末尾だけを Range 取得して `metas.json` を読む。
読み上げに使えるスタイル(`talk` / `streaming_talk`)だけを入れ、歌声だけの VVM(`s0`)は除く。
"""
import io, json, re, sys, urllib.request, zipfile

REPO = "VOICEVOX/voicevox_vvm"
TAG = "0.17.1"
BASE = f"https://github.com/{REPO}/releases/download/{TAG}"
TALK_TYPES = {"talk", "streaming_talk"}
NEMO_CREDIT = "VOICEVOX Nemo"


def get(url: str, headers: dict | None = None, method: str = "GET"):
    req = urllib.request.Request(url, headers=headers or {}, method=method)
    return urllib.request.urlopen(req)


class Ranged(io.RawIOBase):
    """HTTP の Range で読む、シーク可能なファイル風オブジェクト。"""

    def __init__(self, url: str):
        self.url, self.pos = url, 0
        self.size = int(get(url, method="HEAD").headers["Content-Length"])

    def seekable(self): return True
    def readable(self): return True
    def tell(self): return self.pos

    def seek(self, off, whence=0):
        self.pos = off if whence == 0 else self.pos + off if whence == 1 else self.size + off
        return self.pos

    def read(self, n=-1):
        if n < 0:
            n = self.size - self.pos
        n = min(n, self.size - self.pos)
        if n <= 0:
            return b""
        data = get(self.url, {"Range": f"bytes={self.pos}-{self.pos + n - 1}"}).read()
        self.pos += len(data)
        return data


def credits(terms: str) -> dict[str, str]:
    """規約の `## キャラ名` の節から、「…」で示されたクレジット表記を取り出す。"""
    out = {}
    for name, body in re.findall(r"^## (.+?)\n(.*?)(?=^## |\Z)", terms, re.S | re.M):
        m = re.search(r"「([^」]+)」と[^。]{0,8}クレジット", body.replace("\n", ""))
        if m:
            out[name.strip()] = m.group(1)
    return out


def main() -> None:
    rel = json.load(get(f"https://api.github.com/repos/{REPO}/releases/tags/{TAG}"))
    assets = {a["name"]: a for a in rel["assets"]}
    terms = get(f"{BASE}/TERMS.txt").read().decode("utf-8")
    cred = credits(terms)
    vvms = []
    for name in sorted((n for n in assets if n.endswith(".vvm")), key=lambda n: (not n[:-4].isdigit(), int(n[:-4]) if n[:-4].isdigit() else 0, n)):
        z = zipfile.ZipFile(Ranged(f"{BASE}/{name}"))
        chars = []
        for m in json.loads(z.read("metas.json")):
            styles = [{"id": s["id"], "name": s["name"]} for s in m["styles"] if s["type"] in TALK_TYPES]
            if not styles:
                continue
            credit = cred.get(m["name"]) or (NEMO_CREDIT if name == "n0.vvm" else None)
            if credit is None:
                # 規約に個別の節がないキャラは、全体の規約(「VOICEVOX を利用したことがわかるクレジット表記」)に従う。
                credit = f"VOICEVOX:{m['name']}"
                print(f"警告: 規約に個別の節がない。既定のクレジットにした: {m['name']} ({name})", file=sys.stderr)
            chars.append({"name": m["name"], "credit": credit, "styles": styles})
        if not chars:
            continue
        digest = assets[name]["digest"]
        assert digest.startswith("sha256:"), digest
        vvms.append({"id": name[:-4], "size": assets[name]["size"], "sha256": digest[7:], "characters": chars})
    json.dump({"vvm_release": TAG, "vvms": vvms}, sys.stdout, ensure_ascii=False, indent=1)
    print()


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: カタログを生成し、中身を確かめる**

Run:
```sh
cd /home/onodai145/repos/github.com/onodai145/tsumugi
mkdir -p src-tauri/src/tts/voicevox/terms
python3 -I scripts/gen-voicevox-catalog.py > src-tauri/src/tts/voicevox/catalog.json
python3 -I -c "
import json
c=json.load(open('src-tauri/src/tts/voicevox/catalog.json')); v=c['vvms']
print(len(v), round(sum(x['size'] for x in v)/1e9,2), sum(len(ch['styles']) for x in v for ch in x['characters']))
print([x['id'] for x in v][-3:])
"
```
Expected: stderr に警告が 1 行(`雨晴はう (0.vvm)`)。stdout は `27 1.65 137`、`['24', '25', 'n0']`。`s0` が入っていないこと。

- [ ] **Step 3: 規約の写しを置く**

Run:
```sh
cd /home/onodai145/repos/github.com/onodai145/tsumugi
curl -sSfL https://github.com/VOICEVOX/voicevox_vvm/releases/download/0.17.1/TERMS.txt -o src-tauri/src/tts/voicevox/terms/voice-models.txt
D=$HOME/.cache/tsumugi-voicevox-probe/dl && mkdir -p "$D"
curl -sSfL -o "$D/ort-linux-x64.tgz" https://github.com/VOICEVOX/onnxruntime-builder/releases/download/voicevox_onnxruntime-1.17.3/voicevox_onnxruntime-linux-x64-1.17.3.tgz
tar xzf "$D/ort-linux-x64.tgz" -C "$D" voicevox_onnxruntime-linux-x64-1.17.3/TERMS.txt
cp "$D/voicevox_onnxruntime-linux-x64-1.17.3/TERMS.txt" src-tauri/src/tts/voicevox/terms/onnxruntime.txt
wc -c src-tauri/src/tts/voicevox/terms/*.txt
```
Expected: `voice-models.txt` が 15990 バイト、`onnxruntime.txt` が 938 バイト。

- [ ] **Step 4: 実環境の確認用の取得物を、ディスク上に用意する**

(Task 5・12 の `#[ignore]` のテストが使う。`/tmp` ではなく `$HOME/.cache` に置く。)

Run:
```sh
P=$HOME/.cache/tsumugi-voicevox-probe && mkdir -p $P/core $P/ort $P/dict
curl -sSfL -o $P/dl/core.zip https://github.com/VOICEVOX/voicevox_core/releases/download/0.17.1/voicevox_core-linux-x64-0.17.1.zip
curl -sSfL -o $P/dl/dict.tgz https://github.com/r9y9/open_jtalk/releases/download/v1.11.1/open_jtalk_dic_utf_8-1.11.tar.gz
curl -sSfL -o $P/0.vvm https://github.com/VOICEVOX/voicevox_vvm/releases/download/0.17.1/0.vvm
sha256sum $P/dl/core.zip $P/dl/ort-linux-x64.tgz $P/dl/dict.tgz $P/0.vvm
python3 -I -c "import zipfile,sys; zipfile.ZipFile('$P/dl/core.zip').extractall('$P/core')"
tar xzf $P/dl/ort-linux-x64.tgz -C $P/ort && tar xzf $P/dl/dict.tgz -C $P/dict
ls $P/core/*/lib $P/ort/*/lib $P/dict/*/sys.dic
```
Expected: SHA-256 が、Task 2 の表の値と一致する(`core-linux-x64` は `844b49ce…`、ort-linux-x64 は `72b5287f…`、dict は `fe6ba0e4…`、`0.vvm` は `ecd35374…`)。

- [ ] **Step 5: スパイクノートを書く**

`docs/superpowers/plans/2026-11-20-issue-625-stage2-spike-notes.md` に、この plan の冒頭の「plan 作成時に実測して分かったこと」の内容を、表にして残す。項目: ① 取得物の一覧(名前・サイズ・SHA-256・アーカイブ内のパス)、② Linux での実測(ロード時間・合成時間・メモリ・WAV の形式・システムライブラリ)、③ 規約・クレジットの扱い、④ spec との差。さらに、**Task 12 で追記する欄**として、「実アプリ(Xvfb)での確認」「`#[ignore]` の実物のテストの結果」の見出しだけを置く(中身は Task 12 で書く)。

- [ ] **Step 6: spec を訂正する**

`docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md` を、Read してから Edit で次のとおり直す。
  - 「公式の全 VVM(`voicevox_vvm` 0.17.1 の 28 個。1 個あたり約 55〜130 MB、合計約 1.7 GB)」→「公式の VVM のうち、読み上げ用のスタイルを持つ 27 個(`voicevox_vvm` 0.17.1 の `0`〜`25` と `n0`。1 個あたり約 55〜75 MB、合計約 1.65 GB。`s0` は歌声用のスタイルだけなので含めない)」。同じ内容の記述(「28 個」「約 1.7 GB」「28 行」)を、すべて 27 個・約 1.65 GB に直す(`grep -n "28\|1.7 GB" <spec>` で漁る)。
  - 取得物の表の「声」の行の VVM の個数とサイズも同様に直す。
  - 「規約と同意」の「ONNX Runtime 利用規約(`onnxruntime-builder` のリリース本文の `data-voicevox-onnxruntime-terms`)」→「tgz 内の `TERMS.txt`(リリース本文と同じ内容。取得前に読めるよう、アプリに写しを埋め込む)」。同意版は「改行コードを除いた 2 つの規約の写しの SHA-256」と書く。
  - クレジットの段落に「表記は規約の `## キャラ名` の節から取り出す。`n0` は『VOICEVOX Nemo』。規約に個別の節がない `雨晴はう` は『VOICEVOX:雨晴はう』(全体の規約に従う)」を足す。
  - 「声の一覧は、既存の `list_speech_voices` をエンジン別に分岐させて使う」→「声の一覧は、新しいコマンド `list_speech_core_voices`(取得済みの声だけ)で返す。外部エンジンの `list_speech_voices` は URL を引数に取るので、分岐させない」。
  - 「`desktop` のときだけビルド」→「ネイティブの呼び出し(`ffi.rs` の `NativeCore`)だけ `cfg(desktop)`。ほかのモジュールとコマンド・イベントは全環境でビルドする(TS バインディングを環境で変えないため)。Android・Windows arm64・BSD では `supported = false` を返す」。
  - 「ONNX Runtime は 1.17.3 にする」の段落に「CORE 0.17.1 の推奨は 1.23.2(prerelease)だが、パスを明示すれば 1.17.3 で動くことを Linux で確かめた(スパイクノート)」を足す。
  - リスクの最後の段落(`TERMS.txt` の「44 キャラクター前後」)は、「Nemo を含め、全員が『クレジット表記があれば商用・非商用で利用可能』の同じ条件(`雨晴はう` は個別の節がない)」に直す。

- [ ] **Step 7: コミットする**

```sh
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add scripts/gen-voicevox-catalog.py src-tauri/src/tts/voicevox docs/superpowers
git commit -m "feat(tts): 内蔵エンジンのカタログ・規約の写し・スパイクノートを追加し、specを実測値に合わせる(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: カタログと規約の版(`catalog.rs`)

**Files:**
- Create: `src-tauri/src/tts/voicevox/mod.rs`、`src-tauri/src/tts/voicevox/catalog.rs`
- Modify: `src-tauri/src/tts/mod.rs`

**Interfaces:**
- Produces(`catalog.rs`): `CORE_VERSION: &str`、`ORT_VERSION: &str`、`DICT_VERSION: &str`、`ENGINE_SET: &str`、`struct Style { id: u32, name: String }`、`struct Character { name: String, credit: String, styles: Vec<Style> }`、`struct Vvm { id: String, size: u64, sha256: String, characters: Vec<Character> }`、`struct Catalog { vvm_release: String, vvms: Vec<Vvm> }`(`vvm(&self, id) -> Option<&Vvm>`、`voice(&self, style_id: u32) -> Option<(&Vvm, &Character, &Style)>`)、`fn catalog() -> &'static Catalog`、`fn vvm_url(id: &str) -> String`、`struct Asset { url: String, size: u64, sha256: String }`、`struct EngineAssets { core: Asset, ort: Asset, dict: Asset, core_lib: &'static str, ort_lib: &'static str }`、`fn engine_assets_for(os: &str, arch: &str) -> Option<EngineAssets>`、`fn engine_assets() -> Option<EngineAssets>`、`struct Terms { voice_models: &'static str, onnxruntime: &'static str }`、`fn terms() -> Terms`、`fn terms_version() -> String`
- Produces(`tts/mod.rs`): `pub const ERR_NOT_READY`、`ERR_ENGINE_LOAD`、`ERR_UNSUPPORTED`(値は Global Constraints)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/tts/voicevox/mod.rs`(まず骨組みだけ):

```rust
//! 内蔵 VOICEVOX CORE(Issue #625 段階2)。設計は `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`。

pub mod catalog;
```

`src-tauri/src/tts/mod.rs` の `pub mod runtime;` の次の行に `pub mod voicevox;` を足し、エラーコードを `ERR_TEXT_TOO_LONG` の次に足す:

```rust
/// 内蔵エンジンが使えない状態: エンジン一式、または選んだ声の VVM が未取得。
pub const ERR_NOT_READY: &str = "SPEECH_CORE_NOT_READY";
/// 内蔵エンジンのロード・起動に失敗した(次の合成で再試行する)。
pub const ERR_ENGINE_LOAD: &str = "SPEECH_CORE_LOAD_FAILED";
/// この環境では内蔵エンジンを使えない(Android・対応外の CPU など)。
pub const ERR_UNSUPPORTED: &str = "SPEECH_CORE_UNSUPPORTED";
```

`src-tauri/src/tts/voicevox/catalog.rs` の末尾に、テストを書く(実装は Step 3 で上に足す。テストだけの状態で書き始めてよい):

```rust
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
```

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox::catalog 2>&1 | tail -15`
Expected: `cannot find function 'catalog'` などのコンパイルエラー。

- [ ] **Step 3: 実装する**

`catalog.rs` の先頭(テストの上)に書く:

```rust
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
```

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox::catalog 2>&1 | tail -15`
Expected: 7 passed、1 ignored。

- [ ] **Step 5: コミットする**

```sh
git add src-tauri/src/tts
git commit -m "feat(tts): 内蔵VOICEVOX COREのカタログと規約の版を追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ディスク配置と状態の検査(`store.rs`)

**Files:**
- Create: `src-tauri/src/tts/voicevox/store.rs`
- Modify: `src-tauri/src/tts/voicevox/mod.rs`(`pub mod store;`)

**Interfaces:**
- Consumes: `catalog::{catalog, EngineAssets, Vvm, ENGINE_SET}`
- Produces: `struct Store`(`new(root: PathBuf) -> Self`、`engine_dir()`、`engine_tmp_dir()`、`core_lib(&EngineAssets)`、`ort_lib(&EngineAssets)`、`dict_dir()`、`engine_ready(&EngineAssets) -> bool`、`vvm_dir()`、`vvm_path(&str)`、`vvm_ready(&Vvm) -> bool`、`downloads_dir()`、`part_path(file_name: &str, sha256: &str) -> PathBuf`、`part_files(file_name: &str) -> Vec<PathBuf>`、`partial_bytes(file_name: &str, sha256: &str) -> u64`、`clean_stale(&self)`)。すべて `PathBuf` / `bool` を返し、失敗しない(`clean_stale` は失敗を無視する)。

- [ ] **Step 1: 失敗するテストを書く**

`store.rs` の末尾:

```rust
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
        assert!(!s.engine_ready(&a), "READY がなければ、途中のディレクトリは使わない");
        std::fs::write(s.engine_dir().join(READY_FILE), "x").unwrap();
        std::fs::remove_file(s.dict_dir().join("sys.dic")).unwrap();
        assert!(!s.engine_ready(&a), "辞書が欠けていれば未完了");
    }

    #[test]
    fn a_voice_model_is_ready_only_when_its_size_matches() {
        let s = Store::new(temp_root());
        let v = Vvm { id: "0".into(), size: 1000, sha256: "a".repeat(64), characters: vec![] };
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
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "0.vvm.01234567.part");
        assert_eq!(s.partial_bytes("0.vvm", &sha), 0);
        touch(&p, 42);
        assert_eq!(s.partial_bytes("0.vvm", &sha), 42);
        touch(&s.part_path("0.vvm", &"f".repeat(64)), 5);
        touch(&s.part_path("1.vvm", &sha), 5);
        assert_eq!(s.part_files("0.vvm").len(), 2, "別のハッシュの .part も含む。別のファイルのは含まない");
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
        touch(&s.root().join(format!("engine-{ENGINE_SET}.tmp")).join("x"), 1);
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
        assert!(s.root().join("unrelated-file").exists(), "無関係なファイルは触らない");
    }

    #[test]
    fn clean_stale_on_a_missing_root_does_nothing() {
        Store::new(std::env::temp_dir().join(format!("tsumugi_vv_missing_{}", uuid::Uuid::new_v4()))).clean_stale();
    }
}
```

- [ ] **Step 2: テストが失敗することを確かめる**

`mod.rs` に `pub mod store;` を足したうえで、Run: `cd src-tauri && cargo test --lib tts::voicevox::store 2>&1 | tail -10`
Expected: `cannot find type 'Store'` などのコンパイルエラー。

- [ ] **Step 3: 実装する**

`store.rs` の先頭:

```rust
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

    /// エンジン一式が使える状態か。READY(最後に置く)と、必要なファイルがそろっていること。
    pub fn engine_ready(&self, a: &EngineAssets) -> bool {
        let is_file = |p: PathBuf| std::fs::metadata(p).is_ok_and(|m| m.is_file());
        is_file(self.engine_dir().join(READY_FILE)) && is_file(self.core_lib(a)) && is_file(self.ort_lib(a)) && is_file(self.dict_dir().join("sys.dic"))
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
        let Ok(rd) = std::fs::read_dir(self.downloads_dir()) else { return vec![] };
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
        let Ok(rd) = std::fs::read_dir(&self.root) else { return };
        let keep_engine = format!("engine-{ENGINE_SET}");
        let keep_vvm = format!("vvm-{}", catalog().vvm_release);
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let stale = (name.starts_with("engine-") && name != keep_engine) || (name.starts_with("vvm-") && name != keep_vvm);
            if stale && e.path().is_dir() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
        // いまのハッシュの先頭 8 文字(エンジン一式の 3 つ + 全 VVM)。
        let mut tags: Vec<String> = catalog().vvms.iter().map(|v| v.sha256.chars().take(8).collect()).collect();
        if let Some(a) = engine_assets() {
            tags.extend([&a.core, &a.ort, &a.dict].iter().map(|x| x.sha256.chars().take(8).collect::<String>()));
        }
        let Ok(rd) = std::fs::read_dir(self.downloads_dir()) else { return };
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".part") else { continue };
            let current = stem.rsplit('.').next().is_some_and(|tag| tags.iter().any(|t| t == tag));
            if !current {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}
```

注意: `clean_stale` のテストはこのマシンの `engine_assets()`(Linux x86_64)に対応する SHA だけを「いまの」とみなす。テストが使う `catalog().vvms[0].sha256` は VVM のものなので、どの環境でも通る。

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox::store 2>&1 | tail -10`
Expected: 5 passed。

- [ ] **Step 5: コミットする**

```sh
git add src-tauri/src/tts/voicevox
git commit -m "feat(tts): 内蔵エンジンの取得物の置き場と状態の検査を追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```


---

### Task 4: アーカイブの安全な展開(`archive.rs`)と、テスト用のアーカイブ作成

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/tts/voicevox/archive.rs`、`src-tauri/src/tts/voicevox/test_util.rs`
- Modify: `src-tauri/src/tts/voicevox/mod.rs`

**Interfaces:**
- Consumes: `store::Store`、`catalog::{EngineAssets, Vvm}`(`test_util` が使う)
- Produces(`archive.rs`): `enum ExtractError { Unsafe, TooLarge, Missing(String), Format, Io(String) }`(`reason(&self) -> String`、`log_tag(&self) -> &'static str`)、`struct Limits { max_file: u64, max_total: u64 }`(`Copy`)、`enum Select<'a> { Files(&'a [(&'a str, &'a str)]), All }`(`Files` は `(アーカイブ内の、最上位のディレクトリを除いたパス, 出力のファイル名)`)、`fn extract_zip(archive: &Path, dest_dir: &Path, select: Select<'_>, limits: Limits) -> Result<(), ExtractError>`、`fn extract_tgz(…同じ…)`
- Produces(`test_util.rs`、`cfg(test)`): `sha256_hex(&[u8]) -> String`、`make_zip(&[(&str, &[u8])]) -> Vec<u8>`、`make_zip_with_symlink(entries, link: (&str, &str)) -> Vec<u8>`、`make_tgz(entries) -> Vec<u8>`、`make_tgz_with_symlink(entries, link: Option<(&str, &str)>) -> Vec<u8>`、`temp_root() -> PathBuf`、`install_fake_engine(&Store, &EngineAssets)`、`install_fake_vvm(&Store, &Vvm)`

- [ ] **Step 1: 依存を足す**

`src-tauri/Cargo.toml` の `[dependencies]` の最後(`fs4 = "1.1.0"` の次の行)に足す:

```toml
# 内蔵 VOICEVOX CORE(Issue #625 段階2)。取得物の展開(zip・tgz)。全環境でビルドする(Android では使わないが、コマンドは共通)。
zip = { version = "2", default-features = false, features = ["deflate"] }
tar = "0.4"
flate2 = "1"
```

Run: `cd src-tauri && cargo check --lib 2>&1 | tail -5`
Expected: エラーなし(新しい依存の取得で時間がかかる)。

- [ ] **Step 2: テスト用のアーカイブ作成を書く**

`src-tauri/src/tts/voicevox/test_util.rs`:

```rust
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
fn append_raw<W: Write>(b: &mut tar::Builder<W>, name: &str, kind: tar::EntryType, link: Option<&str>, data: &[u8]) {
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
    std::fs::File::create(s.vvm_path(&v.id)).unwrap().set_len(v.size).unwrap();
}
```

`mod.rs` に足す: `pub mod archive;` と、`#[cfg(test)] pub(crate) mod test_util;`。

- [ ] **Step 3: Task 3 の `store.rs` に `write_ready` を足す**

(`test_util` が使う。`Store` の `engine_ready` の上に足す。)

```rust
    /// エンジン一式の完成の印を置く。ディレクトリの中身がそろったあと、最後に呼ぶ。
    pub fn write_ready(dir: &Path) -> std::io::Result<()> {
        std::fs::write(dir.join(READY_FILE), ENGINE_SET)
    }
```

- [ ] **Step 4: 失敗するテストを書く**

`archive.rs` の末尾:

```rust
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
```

- [ ] **Step 5: テストが失敗することを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox::archive 2>&1 | tail -10`
Expected: `cannot find type 'ExtractError'` などのコンパイルエラー。

- [ ] **Step 6: 実装する**

`archive.rs` の先頭:

```rust
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
```

- [ ] **Step 7: テストが通ることを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox 2>&1 | tail -12`
Expected: archive の 12 件と、Task 2・3 の分が通る。

コンパイルエラーが出た場合の手当て(型・メソッド名の差異だけ):
- `tar::Header::as_mut_bytes` が無ければ、`h.as_old_mut().name[..name.len()].copy_from_slice(...)` に替える。
- `zip::ZipFile::is_file` / `unix_mode` / `add_symlink` / `SimpleFileOptions` の名前が違えば、`cargo doc -p zip --no-deps` か docs.rs で、`zip` 2.x の同等の API に替える(`Cargo.toml` のバージョンは `"2"` のまま)。

- [ ] **Step 8: コミットする**

```sh
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/tts/voicevox
git commit -m "feat(tts): 取得物(zip・tgz)の安全な展開を追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: C API の FFI ラッパ(`ffi.rs`)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/tts/voicevox/ffi.rs`
- Modify: `src-tauri/src/tts/voicevox/mod.rs`(`pub mod ffi;`)

**Interfaces:**
- Produces: `enum CoreError { Load(String), Synthesis(String) }`(`Debug + Clone + PartialEq`)、`trait CoreBackend: Send + Sync + 'static { fn load_voice_model(&self, vvm: &Path) -> Result<(), CoreError>; fn audio_query(&self, text: &str, style_id: u32) -> Result<String, CoreError>; fn synthesis(&self, query_json: &str, style_id: u32) -> Result<Vec<u8>, CoreError>; fn unload_all(&self); }`、`fn default_cpu_threads() -> u16`、`#[cfg(desktop)] struct NativeCore`(`fn load(core_lib: &Path, ort_lib: &Path, dict_dir: &Path, cpu_threads: u16) -> Result<NativeCore, CoreError>`、`CoreBackend` を実装)

- [ ] **Step 1: 依存を足す**

`src-tauri/Cargo.toml` の `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]` の最後(`llama-cpp-2 = …` の次の行)に足す:

```toml
# 内蔵 VOICEVOX CORE(Issue #625 段階2)。取得した公式の C API ライブラリを動的ロードする。
libloading = "0.8"
```

- [ ] **Step 2: 失敗するテストを書く**

`ffi.rs` の末尾に、全環境で動く単体テストと、実物の `#[ignore]` のテストを書く:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_thread_count_is_at_least_one_and_at_most_half_of_the_cores() {
        let n = default_cpu_threads() as usize;
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        assert!(n >= 1);
        assert!(n <= cores.max(2) / 2 + 1, "{n} / {cores}");
    }
}

/// 実物の公式ライブラリで合成する(Linux x86_64 のみ)。`TSUMUGI_TEST_PROBE` に、Task 1 の Step 4 で用意した
/// ディレクトリ(`$HOME/.cache/tsumugi-voicevox-probe`)を渡す。
/// `TSUMUGI_TEST_PROBE=$HOME/.cache/tsumugi-voicevox-probe cargo test --lib tts::voicevox::ffi -- --ignored --nocapture`
#[cfg(all(test, desktop, target_os = "linux", target_arch = "x86_64"))]
mod real_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[ignore]
    fn native_core_loads_and_synthesizes_a_real_wav() {
        let p = PathBuf::from(std::env::var("TSUMUGI_TEST_PROBE").expect("TSUMUGI_TEST_PROBE を設定してください"));
        let core = NativeCore::load(
            &p.join("core/voicevox_core-linux-x64-0.17.1/lib/libvoicevox_core.so"),
            &p.join("ort/voicevox_onnxruntime-linux-x64-1.17.3/lib/libvoicevox_onnxruntime.so.1.17.3"),
            &p.join("dict/open_jtalk_dic_utf_8-1.11"),
            default_cpu_threads(),
        )
        .expect("load");
        let vvm = p.join("0.vvm");
        core.load_voice_model(&vvm).expect("load vvm");
        core.load_voice_model(&vvm).expect("2 回目のロードは何もしない");

        let q = core.audio_query("この音声は、ボイスボックスを使用して、出力されています。", 3).expect("audio_query");
        assert!(q.contains("speedScale") && q.contains("volumeScale"), "{}", &q[..q.len().min(200)]);
        let wav = core.synthesis(&q, 3).expect("synthesis");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert!(wav.len() > 100_000, "{}", wav.len());
        let pcm = &wav[44..];
        assert!(pcm.chunks_exact(2).any(|c| i16::from_le_bytes([c[0], c[1]]).abs() > 1000), "無音ではない");

        // 外したあとは、そのスタイルを合成できない。入れ直せば、また合成できる。
        core.unload_all();
        assert!(core.synthesis(&q, 3).is_err());
        core.load_voice_model(&vvm).expect("reload");
        assert!(core.synthesis(&q, 3).is_ok());

        // 存在しない VVM・NUL を含む文でも、落ちずに Err を返す。
        assert!(core.load_voice_model(&p.join("missing.vvm")).is_err());
        assert!(core.audio_query("a\0b", 3).is_err());
    }
}
```

- [ ] **Step 3: テストが失敗することを確かめる**

`mod.rs` に `pub mod ffi;` を足したうえで、Run: `cd src-tauri && cargo test --lib tts::voicevox::ffi 2>&1 | tail -8`
Expected: `cannot find function 'default_cpu_threads'` などのコンパイルエラー。

- [ ] **Step 4: 実装する**

`ffi.rs` の先頭:

```rust
//! VOICEVOX CORE の C API(公式の `libvoicevox_core`)を `libloading` で呼ぶ薄いラッパ(Issue #625 段階2)。
//! **`unsafe` はこのファイルに閉じ込める**。ネイティブの呼び出し(`NativeCore`)は `cfg(desktop)` だけ。
//! 背後の `CoreBackend` trait は全環境にあり、`engine.rs` のテストでは偽の実装に差し替える。
//! 関数の宣言は、公式ヘッダ `voicevox_core.h`(0.17.1)に合わせてある。

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum CoreError {
    /// ライブラリ・ONNX Runtime・辞書・VVM のロード、初期化の失敗(利用者には出さない。ログ用)。
    Load(String),
    /// クエリ作成・合成の失敗。
    Synthesis(String),
}

/// CORE の `Synthesizer` を包む。全メソッドは同期で、`spawn_blocking` から呼ぶ。
pub trait CoreBackend: Send + Sync + 'static {
    /// VVM をロードする。同じファイルがロード済みなら何もしない。
    fn load_voice_model(&self, vvm: &Path) -> Result<(), CoreError>;
    /// テキストから AudioQuery(JSON)を作る。
    fn audio_query(&self, text: &str, style_id: u32) -> Result<String, CoreError>;
    /// AudioQuery(JSON)から WAV のバイト列を作る。
    fn synthesis(&self, query_json: &str, style_id: u32) -> Result<Vec<u8>, CoreError>;
    /// ロード済みの VVM をすべて外す。
    fn unload_all(&self);
}

/// 推論のスレッド数。コア数の半分(最低 1)。CORE の既定(全コア)だと、合成中に UI が重くなるため。
pub fn default_cpu_threads() -> u16 {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    (cores / 2).clamp(1, u16::MAX as usize) as u16
}

#[cfg(desktop)]
pub use native::NativeCore;

#[cfg(desktop)]
mod native {
    use super::{CoreBackend, CoreError};
    use libloading::Library;
    use std::collections::HashMap;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[repr(C)]
    struct LoadOnnxruntimeOptions {
        filename: *const c_char,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct InitializeOptions {
        acceleration_mode: i32,
        cpu_num_threads: u16,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct LoadVoiceModelOptions {
        on_existing: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SynthesisOptions {
        enable_interrogative_upspeak: bool,
    }

    type Code = i32;
    const OK: Code = 0;
    /// `VOICEVOX_ACCELERATION_MODE_CPU`。
    const ACCELERATION_CPU: i32 = 1;

    /// ロードしたライブラリの関数。`Library` はプロセスの終わりまで外さない(下の `api`)ので、`'static` に持てる。
    struct Api {
        error_result_to_message: unsafe extern "C" fn(Code) -> *const c_char,
        onnxruntime_load_once: unsafe extern "C" fn(LoadOnnxruntimeOptions, *mut *const c_void) -> Code,
        open_jtalk_rc_new: unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> Code,
        open_jtalk_rc_delete: unsafe extern "C" fn(*mut c_void),
        synthesizer_new: unsafe extern "C" fn(*const c_void, *const c_void, InitializeOptions, *mut *mut c_void) -> Code,
        synthesizer_delete: unsafe extern "C" fn(*mut c_void),
        voice_model_file_open: unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> Code,
        voice_model_file_id: unsafe extern "C" fn(*const c_void, *mut [u8; 16]),
        voice_model_file_delete: unsafe extern "C" fn(*mut c_void),
        make_default_load_voice_model_options: unsafe extern "C" fn() -> LoadVoiceModelOptions,
        synthesizer_load_voice_model: unsafe extern "C" fn(*const c_void, *const c_void, LoadVoiceModelOptions) -> Code,
        synthesizer_unload_voice_model: unsafe extern "C" fn(*const c_void, *const [u8; 16]) -> Code,
        synthesizer_create_audio_query: unsafe extern "C" fn(*const c_void, *const c_char, u32, *mut *mut c_char) -> Code,
        make_default_synthesis_options: unsafe extern "C" fn() -> SynthesisOptions,
        synthesizer_synthesis: unsafe extern "C" fn(*const c_void, *const c_char, u32, SynthesisOptions, *mut usize, *mut *mut u8) -> Code,
        string_free: unsafe extern "C" fn(*mut c_char),
        bytes_free: unsafe extern "C" fn(*mut u8),
    }

    /// プロセスで 1 回だけロードする。失敗は覚えない(次の呼び出しで再試行する)。
    static API: Mutex<Option<&'static Api>> = Mutex::new(None);

    fn api(core_lib: &Path) -> Result<&'static Api, CoreError> {
        let mut slot = API.lock().unwrap();
        if let Some(a) = *slot {
            return Ok(a);
        }
        // SAFETY: 取得時に SHA-256 で検証した公式のライブラリを、アプリのデータディレクトリの固定の場所から読む。
        let lib = unsafe { Library::new(core_lib) }.map_err(|e| CoreError::Load(format!("library: {e}")))?;
        // プロセスの終わりまで外さない。CORE は ONNX Runtime を内部の static で持つので、外すと再ロードが壊れうる。
        let lib: &'static Library = Box::leak(Box::new(lib));
        macro_rules! sym {
            ($name:literal) => {
                // SAFETY: 宣言は、公式ヘッダ(0.17.1)の関数の型と一致させてある。
                *unsafe { lib.get(concat!($name, "\0").as_bytes()) }
                    .map_err(|e| CoreError::Load(format!("missing symbol {}: {e}", $name)))?
            };
        }
        let api = Api {
            error_result_to_message: sym!("voicevox_error_result_to_message"),
            onnxruntime_load_once: sym!("voicevox_onnxruntime_load_once"),
            open_jtalk_rc_new: sym!("voicevox_open_jtalk_rc_new"),
            open_jtalk_rc_delete: sym!("voicevox_open_jtalk_rc_delete"),
            synthesizer_new: sym!("voicevox_synthesizer_new"),
            synthesizer_delete: sym!("voicevox_synthesizer_delete"),
            voice_model_file_open: sym!("voicevox_voice_model_file_open"),
            voice_model_file_id: sym!("voicevox_voice_model_file_id"),
            voice_model_file_delete: sym!("voicevox_voice_model_file_delete"),
            make_default_load_voice_model_options: sym!("voicevox_make_default_load_voice_model_options"),
            synthesizer_load_voice_model: sym!("voicevox_synthesizer_load_voice_model"),
            synthesizer_unload_voice_model: sym!("voicevox_synthesizer_unload_voice_model"),
            synthesizer_create_audio_query: sym!("voicevox_synthesizer_create_audio_query"),
            make_default_synthesis_options: sym!("voicevox_make_default_synthesis_options"),
            synthesizer_synthesis: sym!("voicevox_synthesizer_synthesis"),
            string_free: sym!("voicevox_string_free"),
            bytes_free: sym!("voicevox_bytes_free"),
        };
        let api: &'static Api = Box::leak(Box::new(api));
        *slot = Some(api);
        Ok(api)
    }

    fn message(api: &Api, code: Code) -> String {
        // SAFETY: 戻り値は、CORE が持つ静的な NUL 終端の文字列(解放不要)。
        let p = unsafe { (api.error_result_to_message)(code) };
        if p.is_null() {
            return format!("code {code}");
        }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }

    fn check(api: &Api, code: Code, what: &str, kind: fn(String) -> CoreError) -> Result<(), CoreError> {
        if code == OK {
            Ok(())
        } else {
            Err(kind(format!("{what}: {}", message(api, code))))
        }
    }

    fn c_path(p: &Path) -> Result<CString, CoreError> {
        p.to_str().and_then(|s| CString::new(s).ok()).ok_or_else(|| CoreError::Load("path is not valid UTF-8 or contains NUL".into()))
    }

    /// ロード済みの `Synthesizer`。**全メソッドは `state` の Mutex で直列化する**(下の `Send` / `Sync` の根拠)。
    pub struct NativeCore {
        api: &'static Api,
        open_jtalk: *mut c_void,
        synthesizer: *mut c_void,
        /// ロード済みの VVM(パス → モデル ID)。ロックは、すべての呼び出しの間、持ち続ける。
        state: Mutex<HashMap<PathBuf, [u8; 16]>>,
    }

    // SAFETY: 生ポインタ(`open_jtalk` と `synthesizer`)へのアクセスは、すべて `state` の Mutex を持った状態で行い、
    // `Drop` は他に参照がないときにだけ走る。
    unsafe impl Send for NativeCore {}
    unsafe impl Sync for NativeCore {}

    impl NativeCore {
        pub fn load(core_lib: &Path, ort_lib: &Path, dict_dir: &Path, cpu_threads: u16) -> Result<Self, CoreError> {
            let api = api(core_lib)?;
            let ort_path = c_path(ort_lib)?;
            let dict = c_path(dict_dir)?;
            let mut ort: *const c_void = std::ptr::null();
            // SAFETY: `ort_path` は、呼び出しの間生きている。`ort` は出力先。ONNX Runtime はプロセスで 1 度だけロードされ、以後は同じものが返る。
            let code = unsafe { (api.onnxruntime_load_once)(LoadOnnxruntimeOptions { filename: ort_path.as_ptr() }, &mut ort) };
            check(api, code, "onnxruntime", CoreError::Load)?;
            let mut open_jtalk: *mut c_void = std::ptr::null_mut();
            // SAFETY: `dict` は呼び出しの間生きている。
            let code = unsafe { (api.open_jtalk_rc_new)(dict.as_ptr(), &mut open_jtalk) };
            check(api, code, "dictionary", CoreError::Load)?;
            let mut synthesizer: *mut c_void = std::ptr::null_mut();
            let options = InitializeOptions { acceleration_mode: ACCELERATION_CPU, cpu_num_threads: cpu_threads };
            // SAFETY: `ort` と `open_jtalk` は、上の呼び出しで得た有効なポインタ。
            let code = unsafe { (api.synthesizer_new)(ort, open_jtalk, options, &mut synthesizer) };
            if let Err(e) = check(api, code, "synthesizer", CoreError::Load) {
                // SAFETY: `open_jtalk` は有効で、まだどこにも渡していない。
                unsafe { (api.open_jtalk_rc_delete)(open_jtalk) };
                return Err(e);
            }
            Ok(Self { api, open_jtalk, synthesizer, state: Mutex::new(HashMap::new()) })
        }
    }

    impl CoreBackend for NativeCore {
        fn load_voice_model(&self, vvm: &Path) -> Result<(), CoreError> {
            let mut loaded = self.state.lock().unwrap();
            if loaded.contains_key(vvm) {
                return Ok(());
            }
            let path = c_path(vvm)?;
            let mut model: *mut c_void = std::ptr::null_mut();
            // SAFETY: `path` は呼び出しの間生きている。
            let code = unsafe { (self.api.voice_model_file_open)(path.as_ptr(), &mut model) };
            check(self.api, code, "voice model", CoreError::Load)?;
            let mut id = [0u8; 16];
            // SAFETY: `model` は、上で開いた有効なハンドル。`id` は 16 バイトの出力先。
            unsafe { (self.api.voice_model_file_id)(model, &mut id) };
            // SAFETY: `synthesizer` と `model` は有効。
            let code = unsafe { (self.api.synthesizer_load_voice_model)(self.synthesizer, model, (self.api.make_default_load_voice_model_options)()) };
            // ハンドルは、ロードのあとは要らない(実測: 閉じても合成できる)。成否にかかわらず閉じる。
            // SAFETY: `model` は有効で、ここで 1 度だけ閉じる。
            unsafe { (self.api.voice_model_file_delete)(model) };
            check(self.api, code, "voice model", CoreError::Load)?;
            loaded.insert(vvm.to_path_buf(), id);
            Ok(())
        }

        fn audio_query(&self, text: &str, style_id: u32) -> Result<String, CoreError> {
            let _serial = self.state.lock().unwrap();
            let text = CString::new(text).map_err(|_| CoreError::Synthesis("text contains NUL".into()))?;
            let mut out: *mut c_char = std::ptr::null_mut();
            // SAFETY: `text` は呼び出しの間生きている。
            let code = unsafe { (self.api.synthesizer_create_audio_query)(self.synthesizer, text.as_ptr(), style_id, &mut out) };
            check(self.api, code, "audio query", CoreError::Synthesis)?;
            // SAFETY: 成功時の `out` は、CORE が確保した NUL 終端の UTF-8 文字列。コピーしてから `string_free` で解放する。
            let s = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
            unsafe { (self.api.string_free)(out) };
            Ok(s)
        }

        fn synthesis(&self, query_json: &str, style_id: u32) -> Result<Vec<u8>, CoreError> {
            let _serial = self.state.lock().unwrap();
            let q = CString::new(query_json).map_err(|_| CoreError::Synthesis("query contains NUL".into()))?;
            let mut len: usize = 0;
            let mut wav: *mut u8 = std::ptr::null_mut();
            // SAFETY: `q` は呼び出しの間生きている。
            let code = unsafe {
                (self.api.synthesizer_synthesis)(self.synthesizer, q.as_ptr(), style_id, (self.api.make_default_synthesis_options)(), &mut len, &mut wav)
            };
            check(self.api, code, "synthesis", CoreError::Synthesis)?;
            // SAFETY: 成功時の `wav` は、CORE が確保した `len` バイト。コピーしてから `bytes_free` で解放する。
            let bytes = unsafe { std::slice::from_raw_parts(wav, len) }.to_vec();
            unsafe { (self.api.bytes_free)(wav) };
            Ok(bytes)
        }

        fn unload_all(&self) {
            let mut loaded = self.state.lock().unwrap();
            for (_, id) in loaded.drain() {
                // SAFETY: `synthesizer` は有効で、`id` は、ロード時に CORE から得たモデル ID。失敗は無視する。
                let _ = unsafe { (self.api.synthesizer_unload_voice_model)(self.synthesizer, &id) };
            }
        }
    }

    impl Drop for NativeCore {
        fn drop(&mut self) {
            self.unload_all();
            // SAFETY: どちらも有効で、ここで 1 度だけ解放する(他に参照はない)。
            unsafe {
                (self.api.synthesizer_delete)(self.synthesizer);
                (self.api.open_jtalk_rc_delete)(self.open_jtalk);
            }
        }
    }
}
```

- [ ] **Step 5: ビルドと、全環境のテストを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox::ffi 2>&1 | tail -8`
Expected: コンパイルが通り、`the_default_thread_count…` が通る。実物のテストは `ignored`。

コンパイルエラーが出た場合の手当て: `lib.get(…)` の型推論が通らなければ、`sym!` の中を `*unsafe { lib.get::<_>(…) }` ではなく、フィールドの型を `let f: unsafe extern "C" fn(…) -> … = *…;` で明示する形に替える(関数の型は `Api` のフィールドと同じ)。

- [ ] **Step 6: 実物のライブラリで確かめる**

Run:
```sh
cd src-tauri && TSUMUGI_TEST_PROBE=$HOME/.cache/tsumugi-voicevox-probe cargo test --lib tts::voicevox::ffi -- --ignored --nocapture 2>&1 | tail -12
```
Expected: `native_core_loads_and_synthesizes_a_real_wav` が通る(約 2〜4 秒)。ここで失敗したら、**先へ進まず**、ヘッダ(`$HOME/.cache/tsumugi-voicevox-probe/core/voicevox_core-linux-x64-0.17.1/include/voicevox_core.h`)と `Api` の宣言を突き合わせる。

- [ ] **Step 7: コミットする**

```sh
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/tts/voicevox
git commit -m "feat(tts): VOICEVOX COREのC APIを動的ロードするFFIラッパを追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: `CoreEngine`(`engine.rs`)

**Files:**
- Create: `src-tauri/src/tts/voicevox/engine.rs`
- Modify: `src-tauri/src/tts/voicevox/mod.rs`(`pub mod engine;`)

**Interfaces:**
- Consumes: `store::Store`、`catalog::{catalog, EngineAssets}`、`ffi::{CoreBackend, CoreError}`、`tts::engine::{SpeechEngine, SynthParams, Voice}`、`tts::{invalid, ERR_*}`、`test_util::{install_fake_engine, install_fake_vvm, temp_root}`
- Produces: `type BackendFactory = Arc<dyn Fn() -> Result<Arc<dyn CoreBackend>, CoreError> + Send + Sync>`、`struct CoreEngine`(`Clone`。`new(store: Arc<Store>, assets: Option<EngineAssets>, factory: BackendFactory) -> Self`、`unload(&self)`、`unload_if_idle(&self, now: Instant, idle: Duration) -> bool`、`is_loaded(&self) -> bool`、`SpeechEngine` を実装)、`const IDLE_UNLOAD: Duration`(600 秒)、`const SWEEP_INTERVAL: Duration`(30 秒)、`async fn run_idle_sweeper(engine: CoreEngine, idle: Duration, interval: Duration)`

- [ ] **Step 1: 失敗するテストを書く**

`engine.rs` の末尾:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::tts::voicevox::catalog::engine_assets_for;
    use crate::tts::voicevox::test_util::{install_fake_engine, install_fake_vvm, temp_root};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    #[derive(Default)]
    struct Fake {
        loaded: Mutex<Vec<PathBuf>>,
        queries: Mutex<Vec<String>>,
        fail_synthesis: AtomicBool,
        panic_in_synthesis: AtomicBool,
    }

    impl CoreBackend for Fake {
        fn load_voice_model(&self, vvm: &Path) -> std::result::Result<(), CoreError> {
            self.loaded.lock().unwrap().push(vvm.to_path_buf());
            Ok(())
        }
        fn audio_query(&self, text: &str, style_id: u32) -> std::result::Result<String, CoreError> {
            Ok(format!(r#"{{"speedScale":1.0,"volumeScale":1.0,"text":"{text}","style":{style_id}}}"#))
        }
        fn synthesis(&self, query_json: &str, _style_id: u32) -> std::result::Result<Vec<u8>, CoreError> {
            if self.panic_in_synthesis.load(Ordering::SeqCst) {
                panic!("boom");
            }
            if self.fail_synthesis.load(Ordering::SeqCst) {
                return Err(CoreError::Synthesis("fake".into()));
            }
            self.queries.lock().unwrap().push(query_json.to_string());
            Ok(b"RIFFfake".to_vec())
        }
        fn unload_all(&self) {}
    }

    struct Setup {
        engine: CoreEngine,
        fake: Arc<Fake>,
        factory_calls: Arc<AtomicUsize>,
        store: Arc<Store>,
    }

    fn setup(installed: bool) -> Setup {
        let store = Arc::new(Store::new(temp_root()));
        let assets = engine_assets_for("linux", "x86_64");
        if installed {
            install_fake_engine(&store, assets.as_ref().unwrap());
            install_fake_vvm(&store, catalog().vvm("0").unwrap());
        }
        let fake = Arc::new(Fake::default());
        let factory_calls = Arc::new(AtomicUsize::new(0));
        let (f, c) = (fake.clone(), factory_calls.clone());
        let factory: BackendFactory = Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(f.clone() as Arc<dyn CoreBackend>)
        });
        Setup { engine: CoreEngine::new(store.clone(), assets, factory), fake, factory_calls, store }
    }

    fn params() -> SynthParams {
        SynthParams { speed_scale: 1.5, volume: 0.8 }
    }

    fn code(r: Result<Vec<u8>>) -> String {
        match r {
            Err(Error::Invalid(m)) => m,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn it_synthesizes_with_the_speed_and_volume_applied() {
        let s = setup(true);
        let wav = s.engine.synthesize("こんにちは", "3", &params()).await.unwrap();
        assert_eq!(wav, b"RIFFfake");
        assert_eq!(*s.fake.loaded.lock().unwrap(), vec![s.store.vvm_path("0")], "声が入っている VVM をロードする");
        let q: serde_json::Value = serde_json::from_str(&s.fake.queries.lock().unwrap()[0]).unwrap();
        assert_eq!(q["speedScale"].as_f64().unwrap(), 1.5);
        assert!((q["volumeScale"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert_eq!(q["style"], 3);
        assert_eq!(q["text"], "こんにちは");
    }

    #[tokio::test]
    async fn a_bad_voice_id_is_no_voice_and_never_touches_the_backend() {
        let s = setup(true);
        for id in ["", "  ", "abc", "-1", "3.5", "99999999999", "99999"] {
            assert_eq!(code(s.engine.synthesize("a", id, &params()).await), crate::tts::ERR_NO_VOICE, "{id:?}");
        }
        assert_eq!(s.factory_calls.load(Ordering::SeqCst), 0, "ロードもしない");
    }

    #[tokio::test]
    async fn a_missing_engine_or_voice_model_is_not_ready() {
        let s = setup(false);
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_NOT_READY);
        // エンジン一式だけ取得済みで、その声の VVM がない。
        install_fake_engine(&s.store, &engine_assets_for("linux", "x86_64").unwrap());
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_NOT_READY);
        assert_eq!(s.factory_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_unsupported_platform_reports_unsupported() {
        let store = Arc::new(Store::new(temp_root()));
        let factory: BackendFactory = Arc::new(|| Err(CoreError::Load("never".into())));
        let engine = CoreEngine::new(store, None, factory);
        assert_eq!(code(engine.synthesize("a", "3", &params()).await), crate::tts::ERR_UNSUPPORTED);
        assert!(engine.voices().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_load_failure_is_reported_and_not_remembered() {
        let store = Arc::new(Store::new(temp_root()));
        let assets = engine_assets_for("linux", "x86_64");
        install_fake_engine(&store, assets.as_ref().unwrap());
        install_fake_vvm(&store, catalog().vvm("0").unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fake = Arc::new(Fake::default());
        let f = fake.clone();
        let factory: BackendFactory = Arc::new(move || {
            if c.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(CoreError::Load("first time fails".into()))
            } else {
                Ok(f.clone() as Arc<dyn CoreBackend>)
            }
        });
        let engine = CoreEngine::new(store, assets, factory);
        assert_eq!(code(engine.synthesize("a", "3", &params()).await), crate::tts::ERR_ENGINE_LOAD);
        assert!(engine.synthesize("a", "3", &params()).await.is_ok(), "次の合成で再試行する");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_synthesis_failure_is_a_bad_response_and_a_panic_does_not_escape() {
        let s = setup(true);
        s.fake.fail_synthesis.store(true, Ordering::SeqCst);
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_BAD_RESPONSE);
        s.fake.fail_synthesis.store(false, Ordering::SeqCst);
        s.fake.panic_in_synthesis.store(true, Ordering::SeqCst);
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_ENGINE_LOAD, "ネイティブ側の panic は Err にする");
        s.fake.panic_in_synthesis.store(false, Ordering::SeqCst);
        assert!(s.engine.synthesize("a", "3", &params()).await.is_ok(), "panic のあとも使える");
    }

    #[tokio::test]
    async fn voices_lists_only_the_installed_voice_models() {
        let s = setup(true);
        let voices = s.engine.voices().await.unwrap();
        assert!(voices.iter().any(|v| v.id == "3" && v.label == "ずんだもん（ノーマル）"));
        assert!(!voices.iter().any(|v| v.id == "14"), "VVM 1 は未取得");
        assert_eq!(voices.len(), catalog().vvm("0").unwrap().characters.iter().map(|c| c.styles.len()).sum::<usize>());
    }

    #[tokio::test]
    async fn the_backend_is_unloaded_when_idle_and_loaded_again_on_demand() {
        let s = setup(true);
        let idle = Duration::from_secs(600);
        assert!(!s.engine.unload_if_idle(Instant::now() + idle * 2, idle), "まだ何もロードしていない");
        s.engine.synthesize("a", "3", &params()).await.unwrap();
        assert!(s.engine.is_loaded());
        assert!(!s.engine.unload_if_idle(Instant::now(), idle), "使った直後は解放しない");
        assert!(s.engine.unload_if_idle(Instant::now() + idle + Duration::from_secs(1), idle));
        assert!(!s.engine.is_loaded());
        s.engine.synthesize("a", "3", &params()).await.unwrap();
        assert_eq!(s.factory_calls.load(Ordering::SeqCst), 2, "必要になったら、また作る");
        s.engine.unload();
        assert!(!s.engine.is_loaded());
    }
}
```

- [ ] **Step 2: テストが失敗することを確かめる**

`mod.rs` に `pub mod engine;` を足したうえで、Run: `cd src-tauri && cargo test --lib tts::voicevox::engine 2>&1 | tail -8`
Expected: `cannot find type 'CoreEngine'` などのコンパイルエラー。

- [ ] **Step 3: 実装する**

`engine.rs` の先頭:

```rust
//! 内蔵 VOICEVOX CORE の `SpeechEngine`(Issue #625 段階2)。声の ID(スタイル ID)から VVM を引き、
//! 必要なものだけをロードして合成する。ネイティブの呼び出しは `CoreBackend`(`ffi.rs`)の背後。

use super::catalog::{catalog, EngineAssets};
use super::ffi::{CoreBackend, CoreError};
use super::store::Store;
use crate::error::Result;
use crate::tts::engine::{SpeechEngine, SynthParams, Voice};
use crate::tts::{invalid, ERR_BAD_RESPONSE, ERR_ENGINE_LOAD, ERR_NOT_READY, ERR_NO_VOICE, ERR_UNSUPPORTED};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 使われなくなってから、ロード済みのモデルを解放するまで(翻訳モデルと同じ)。
pub const IDLE_UNLOAD: Duration = Duration::from_secs(600);
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

pub type BackendFactory = Arc<dyn Fn() -> std::result::Result<Arc<dyn CoreBackend>, CoreError> + Send + Sync>;

struct Inner {
    store: Arc<Store>,
    /// `None` は、対応しない環境。
    assets: Option<EngineAssets>,
    factory: BackendFactory,
    backend: Mutex<Option<Arc<dyn CoreBackend>>>,
    last_used: Mutex<Instant>,
    in_flight: AtomicUsize,
}

/// 合成の間だけ、解放を止める。
struct InFlight<'a>(&'a Inner);

impl<'a> InFlight<'a> {
    fn new(inner: &'a Inner) -> Self {
        inner.in_flight.fetch_add(1, Ordering::SeqCst);
        *inner.last_used.lock().unwrap() = Instant::now();
        Self(inner)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        *self.0.last_used.lock().unwrap() = Instant::now();
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Inner {
    /// ロード済みのバックエンドを返す。なければ作る。失敗は覚えない(次の呼び出しで再試行する)。
    fn backend(&self) -> Result<Arc<dyn CoreBackend>> {
        let mut slot = self.backend.lock().unwrap();
        if let Some(b) = slot.as_ref() {
            return Ok(b.clone());
        }
        let b = (self.factory)().map_err(|e| {
            log::warn!("voicevox core could not be loaded: {e:?}");
            invalid(ERR_ENGINE_LOAD)
        })?;
        *slot = Some(b.clone());
        Ok(b)
    }
}

#[derive(Clone)]
pub struct CoreEngine {
    inner: Arc<Inner>,
}

impl CoreEngine {
    pub fn new(store: Arc<Store>, assets: Option<EngineAssets>, factory: BackendFactory) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                assets,
                factory,
                backend: Mutex::new(None),
                last_used: Mutex::new(Instant::now()),
                in_flight: AtomicUsize::new(0),
            }),
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.inner.backend.lock().unwrap().is_some()
    }

    /// ロード済みのモデルを解放する。ファイルを消す前に呼ぶ(Windows は使用中のファイルを消せない)。
    /// 合成の最中なら、その合成が終わったときに解放される(バックエンドの最後の参照が落ちるとき)。
    pub fn unload(&self) {
        let b = self.inner.backend.lock().unwrap().take();
        drop(b);
    }

    /// `idle` 以上使われていなければ解放する。合成の最中は解放しない。解放したら `true`。
    pub fn unload_if_idle(&self, now: Instant, idle: Duration) -> bool {
        if self.inner.in_flight.load(Ordering::SeqCst) > 0 {
            return false;
        }
        let last = *self.inner.last_used.lock().unwrap();
        let mut slot = self.inner.backend.lock().unwrap();
        if slot.is_some() && now.saturating_duration_since(last) >= idle {
            *slot = None;
            true
        } else {
            false
        }
    }
}

/// 一定間隔でアイドルを見て、解放する。アプリの起動時に 1 つだけ動かす。
pub async fn run_idle_sweeper(engine: CoreEngine, idle: Duration, interval: Duration) {
    loop {
        tokio::time::sleep(interval).await;
        engine.unload_if_idle(Instant::now(), idle);
    }
}

fn core_err(e: CoreError) -> crate::error::Error {
    match e {
        CoreError::Load(m) => {
            log::warn!("voicevox core load error: {m}");
            invalid(ERR_ENGINE_LOAD)
        }
        CoreError::Synthesis(m) => {
            log::warn!("voicevox core synthesis error: {m}");
            invalid(ERR_BAD_RESPONSE)
        }
    }
}

/// AudioQuery に、話速と音量を設定する。
fn apply_params(query_json: &str, p: &SynthParams) -> Result<String> {
    let mut q: serde_json::Value = serde_json::from_str(query_json).map_err(|_| invalid(ERR_BAD_RESPONSE))?;
    let obj = q.as_object_mut().ok_or_else(|| invalid(ERR_BAD_RESPONSE))?;
    obj.insert("speedScale".into(), serde_json::json!(p.speed_scale));
    obj.insert("volumeScale".into(), serde_json::json!(p.volume));
    serde_json::to_string(&q).map_err(|_| invalid(ERR_BAD_RESPONSE))
}

#[async_trait]
impl SpeechEngine for CoreEngine {
    /// 取得済みの声だけ。
    async fn voices(&self) -> Result<Vec<Voice>> {
        let mut out = vec![];
        for v in &catalog().vvms {
            if !self.inner.store.vvm_ready(v) {
                continue;
            }
            for c in &v.characters {
                for s in &c.styles {
                    out.push(Voice { id: s.id.to_string(), label: format!("{}（{}）", c.name, s.name) });
                }
            }
        }
        Ok(out)
    }

    async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>> {
        // 声の ID は、スタイル ID の数値。空・数値でない・カタログにない ID は、ロードより前に弾く。
        let style: u32 = voice_id.trim().parse().map_err(|_| invalid(ERR_NO_VOICE))?;
        let (vvm, _, _) = catalog().voice(style).ok_or_else(|| invalid(ERR_NO_VOICE))?;
        let assets = self.inner.assets.as_ref().ok_or_else(|| invalid(ERR_UNSUPPORTED))?;
        if !self.inner.store.engine_ready(assets) || !self.inner.store.vvm_ready(vvm) {
            return Err(invalid(ERR_NOT_READY));
        }
        let vvm_path = self.inner.store.vvm_path(&vvm.id);
        let inner = self.inner.clone();
        let (text, p) = (text.to_string(), *p);
        // ロードも合成も CPU を使う同期処理なので、ランタイムを止めないよう、専用のスレッドで行う。
        let joined = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let _busy = InFlight::new(&inner);
            let backend = inner.backend()?;
            backend.load_voice_model(&vvm_path).map_err(core_err)?;
            let query = backend.audio_query(&text, style).map_err(core_err)?;
            let query = apply_params(&query, &p)?;
            backend.synthesis(&query, style).map_err(core_err)
        })
        .await;
        match joined {
            Ok(r) => r,
            // ネイティブ側の panic。アプリを落とさず、失敗として扱う(次の合成で、バックエンドを作り直す)。
            Err(_) => {
                self.unload();
                Err(invalid(ERR_ENGINE_LOAD))
            }
        }
    }
}
```

`SynthParams` は `Copy` なので、`*p` で取り出せる。

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox 2>&1 | tail -8`
Expected: engine の 8 件を含め、voicevox の全テストが通る。

- [ ] **Step 5: コミットする**

```sh
git add src-tauri/src/tts/voicevox
git commit -m "feat(tts): 内蔵VOICEVOX COREのSpeechEngineを追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: 取得・削除・進捗の状態機械(`packs.rs`)

**Files:**
- Create: `src-tauri/src/tts/voicevox/packs.rs`
- Modify: `src-tauri/src/tts/voicevox/mod.rs`(`pub mod packs;`)

**Interfaces:**
- Consumes: `archive::{extract_tgz, extract_zip, ExtractError, Limits, Select}`、`catalog::{catalog, terms_version, vvm_url, Asset, EngineAssets}`、`engine::CoreEngine`(`unload()`)、`store::Store`、`translate::download::{download, DownloadError, ModelSpec, Phase}`、`translate::model_store::{available_space, LocalModelStatus}`、`test_util::*`
- Produces: `type Notify = Arc<dyn Fn() + Send + Sync>`、`enum PackKey { Engine, Voice(String) }`(`Clone + Debug + PartialEq + Eq + Hash`。`PackKey::parse(&str) -> Result<PackKey>`: `"engine"` か、カタログにある VVM の ID)、`struct PackInfo { status: LocalModelStatus, size_bytes: u64, partial_bytes: u64 }`、`struct StyleInfo { id: String, name: String }`、`struct CharacterInfo { name: String, credit: String, styles: Vec<StyleInfo> }`、`struct VoicePackInfo { id: String, pack: PackInfo, characters: Vec<CharacterInfo> }`、`struct SpeechCoreInfo { supported: bool, terms_version: String, terms_accepted: bool, engine: PackInfo, voices: Vec<VoicePackInfo> }`(いずれも `Serialize + Deserialize + specta::Type`、`camelCase`)、`struct Packs`(`new(store: Arc<Store>, assets: Option<EngineAssets>, http: reqwest::Client, engine: CoreEngine) -> Self`、`snapshot(&self) -> SpeechCoreInfo`(`terms_accepted` は `false`。呼び出し側が埋める)、`start(self: &Arc<Self>, key: PackKey, notify: Notify) -> Result<()>`、`cancel(&self)`、`async delete(&self, key: PackKey) -> Result<()>`、`is_running(&self) -> bool`)

- [ ] **Step 1: 失敗するテストを書く**

`packs.rs` の末尾:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::voicevox::catalog::{engine_assets_for, Asset};
    use crate::tts::voicevox::ffi::CoreError;
    use crate::tts::voicevox::test_util::{install_fake_engine, install_fake_vvm, make_tgz, make_tgz_with_symlink, make_zip, sha256_hex, temp_root};
    use std::sync::atomic::AtomicUsize;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct T {
        packs: Arc<Packs>,
        store: Arc<Store>,
        notified: Arc<AtomicUsize>,
        notify: Notify,
    }

    fn engine_stub(store: &Arc<Store>) -> CoreEngine {
        CoreEngine::new(store.clone(), None, Arc::new(|| Err(CoreError::Load("n/a".into()))))
    }

    fn build(assets: Option<EngineAssets>, voice: Option<(&str, ModelSpec)>) -> T {
        let store = Arc::new(Store::new(temp_root()));
        let mut packs = Packs::new(store.clone(), assets, reqwest::Client::new(), engine_stub(&store));
        if let Some((id, spec)) = voice {
            packs.override_voice_spec(id, spec);
        }
        let notified = Arc::new(AtomicUsize::new(0));
        let n = notified.clone();
        let notify: Notify = Arc::new(move || {
            n.fetch_add(1, Ordering::SeqCst);
        });
        T { packs: Arc::new(packs), store, notified, notify }
    }

    fn voice_spec(server: &MockServer, body: &[u8]) -> ModelSpec {
        ModelSpec { url: format!("{}/0.vvm", server.uri()), size: body.len() as u64, sha256: sha256_hex(body) }
    }

    async fn serve(server: &MockServer, p: &str, body: Vec<u8>) {
        Mock::given(method("GET")).and(path(p.to_string())).respond_with(ResponseTemplate::new(200).set_body_bytes(body)).mount(server).await;
    }

    async fn wait_idle(p: &Arc<Packs>) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while p.is_running() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("取得が終わらない");
    }

    fn engine_status(t: &T) -> LocalModelStatus {
        t.packs.snapshot().engine.status
    }

    fn voice_status(t: &T, id: &str) -> LocalModelStatus {
        t.packs.snapshot().voices.into_iter().find(|v| v.id == id).unwrap().pack.status
    }

    fn archives() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let core = make_zip(&[("voicevox_core-linux-x64-0.17.1/lib/libvoicevox_core.so", b"CORE"), ("voicevox_core-linux-x64-0.17.1/README.txt", b"r")]);
        let ort = make_tgz_with_symlink(
            &[
                ("voicevox_onnxruntime-linux-x64-1.17.3/lib/libvoicevox_onnxruntime.so.1.17.3", b"ORT"),
                ("voicevox_onnxruntime-linux-x64-1.17.3/TERMS.txt", b"t"),
            ],
            Some(("voicevox_onnxruntime-linux-x64-1.17.3/lib/libvoicevox_onnxruntime.so", "libvoicevox_onnxruntime.so.1.17.3")),
        );
        let dict = make_tgz(&[("open_jtalk_dic_utf_8-1.11/sys.dic", b"SYS"), ("open_jtalk_dic_utf_8-1.11/char.bin", b"C")]);
        (core, ort, dict)
    }

    fn fake_assets(uri: &str, core: &[u8], ort: &[u8], dict: &[u8]) -> EngineAssets {
        let mut a = engine_assets_for("linux", "x86_64").unwrap();
        let asset = |p: &str, b: &[u8]| Asset { url: format!("{uri}/{p}"), size: b.len() as u64, sha256: sha256_hex(b) };
        a.core = asset("core.zip", core);
        a.ort = asset("ort.tgz", ort);
        a.dict = asset("dict.tgz", dict);
        a
    }

    async fn serve_engine(server: &MockServer, core: &[u8], ort: &[u8], dict: &[u8]) {
        serve(server, "/core.zip", core.to_vec()).await;
        serve(server, "/ort.tgz", ort.to_vec()).await;
        serve(server, "/dict.tgz", dict.to_vec()).await;
    }

    #[tokio::test]
    async fn a_voice_pack_downloads_verifies_and_becomes_ready() {
        let server = MockServer::start().await;
        let body: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        serve(&server, "/0.vvm", body.clone()).await;
        let t = build(None, Some(("0", voice_spec(&server, &body))));
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::NotInstalled);

        t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;

        assert_eq!(voice_status(&t, "0"), LocalModelStatus::Ready);
        assert_eq!(std::fs::read(t.store.vvm_path("0")).unwrap(), body);
        assert!(t.store.part_files("0.vvm").is_empty(), ".part は残らない");
        assert!(t.notified.load(Ordering::SeqCst) >= 2, "開始と完了で通知する");
    }

    #[tokio::test]
    async fn a_hash_mismatch_fails_and_leaves_nothing() {
        let server = MockServer::start().await;
        let body = vec![1u8; 2048];
        serve(&server, "/0.vvm", body.clone()).await;
        let mut spec = voice_spec(&server, &body);
        spec.sha256 = "a".repeat(64);
        let t = build(None, Some(("0", spec)));
        t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert!(matches!(voice_status(&t, "0"), LocalModelStatus::Failed { .. }));
        assert!(!t.store.vvm_path("0").exists());
        assert!(t.store.part_files("0.vvm").is_empty());
    }

    #[tokio::test]
    async fn a_failed_pack_can_be_retried() {
        let server = MockServer::start().await;
        let body = vec![5u8; 1024];
        Mock::given(method("GET")).and(path("/0.vvm")).respond_with(ResponseTemplate::new(500)).up_to_n_times(1).mount(&server).await;
        serve(&server, "/0.vvm", body.clone()).await;
        let t = build(None, Some(("0", voice_spec(&server, &body))));
        t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert!(matches!(voice_status(&t, "0"), LocalModelStatus::Failed { ref reason } if reason.contains("500")), "{:?}", voice_status(&t, "0"));
        t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::Ready);
    }

    #[tokio::test]
    async fn unknown_packs_a_second_start_and_an_already_ready_pack_are_rejected() {
        assert!(PackKey::parse("nope").is_err());
        assert!(PackKey::parse("../0").is_err());
        assert!(PackKey::parse("s0").is_err(), "歌声だけの VVM はない");
        assert_eq!(PackKey::parse("engine").unwrap(), PackKey::Engine);
        assert_eq!(PackKey::parse("n0").unwrap(), PackKey::Voice("n0".into()));

        let server = MockServer::start().await;
        let body = vec![9u8; 1024];
        Mock::given(method("GET")).and(path("/0.vvm")).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()).set_delay(Duration::from_millis(400))).mount(&server).await;
        let t = build(None, Some(("0", voice_spec(&server, &body))));
        t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).unwrap();
        assert!(t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).is_err(), "同時に走るのは 1 つ");
        assert!(t.packs.delete(PackKey::Voice("0".into())).await.is_err(), "取得中は削除できない");
        wait_idle(&t.packs).await;
        assert!(t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).is_err(), "取得済みは拒否");
    }

    #[tokio::test]
    async fn cancel_returns_to_not_installed() {
        let server = MockServer::start().await;
        let body = vec![3u8; 1024];
        Mock::given(method("GET")).and(path("/0.vvm")).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()).set_delay(Duration::from_secs(5))).mount(&server).await;
        let t = build(None, Some(("0", voice_spec(&server, &body))));
        t.packs.start(PackKey::Voice("0".into()), t.notify.clone()).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(matches!(voice_status(&t, "0"), LocalModelStatus::Downloading { .. }));
        t.packs.cancel();
        wait_idle(&t.packs).await;
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::NotInstalled, "キャンセルは失敗ではない");
    }

    #[tokio::test]
    async fn the_engine_pack_downloads_extracts_only_what_it_needs_and_commits() {
        let server = MockServer::start().await;
        let (core, ort, dict) = archives();
        serve_engine(&server, &core, &ort, &dict).await;
        let a = fake_assets(&server.uri(), &core, &ort, &dict);
        let t = build(Some(a.clone()), None);

        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;

        assert_eq!(engine_status(&t), LocalModelStatus::Ready, "{:?}", engine_status(&t));
        assert!(t.store.engine_ready(&a));
        assert_eq!(std::fs::read(t.store.core_lib(&a)).unwrap(), b"CORE");
        assert_eq!(std::fs::read(t.store.ort_lib(&a)).unwrap(), b"ORT");
        assert_eq!(std::fs::read(t.store.dict_dir().join("sys.dic")).unwrap(), b"SYS");
        assert!(!t.store.engine_dir().join("core/README.txt").exists(), "必要なファイル以外は取り出さない");
        assert!(!t.store.engine_dir().join("ort/libvoicevox_onnxruntime.so").exists(), "シンボリックリンクは取り出さない");
        assert!(!t.store.engine_tmp_dir().exists());
        assert!(std::fs::read_dir(t.store.downloads_dir()).unwrap().next().is_none(), "アーカイブも .part も残らない");
    }

    #[tokio::test]
    async fn a_malicious_engine_archive_fails_and_leaves_no_directory() {
        let server = MockServer::start().await;
        let (core, ort, _) = archives();
        let evil = make_tgz(&[("top/ok.bin", b"x"), ("top/../evil.txt", b"E")]);
        serve_engine(&server, &core, &ort, &evil).await;
        let a = fake_assets(&server.uri(), &core, &ort, &evil);
        let t = build(Some(a.clone()), None);
        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        match engine_status(&t) {
            LocalModelStatus::Failed { reason } => assert!(reason.contains("安全"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(!t.store.engine_tmp_dir().exists() && !t.store.engine_dir().exists());
        assert!(!t.store.root().join("evil.txt").exists() && !t.store.engine_tmp_dir().join("evil.txt").exists());
    }

    #[tokio::test]
    async fn an_engine_archive_missing_the_library_fails() {
        let server = MockServer::start().await;
        let (_, ort, dict) = archives();
        let core = make_zip(&[("voicevox_core-linux-x64-0.17.1/README.txt", b"r")]);
        serve_engine(&server, &core, &ort, &dict).await;
        let t = build(Some(fake_assets(&server.uri(), &core, &ort, &dict)), None);
        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert!(matches!(engine_status(&t), LocalModelStatus::Failed { .. }));
        assert!(!t.store.engine_dir().exists());
    }

    #[tokio::test]
    async fn the_engine_pack_is_rejected_where_unsupported() {
        let t = build(None, None);
        let snap = t.packs.snapshot();
        assert!(!snap.supported);
        assert_eq!(snap.engine.status, LocalModelStatus::NotInstalled);
        match t.packs.start(PackKey::Engine, t.notify.clone()) {
            Err(Error::Invalid(m)) => assert_eq!(m, ERR_UNSUPPORTED),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn delete_removes_the_files_and_returns_to_not_installed() {
        let a = engine_assets_for("linux", "x86_64").unwrap();
        let t = build(Some(a.clone()), None);
        install_fake_engine(&t.store, &a);
        install_fake_vvm(&t.store, catalog().vvm("0").unwrap());
        std::fs::create_dir_all(t.store.downloads_dir()).unwrap();
        std::fs::write(t.store.part_path("0.vvm", &catalog().vvm("0").unwrap().sha256), b"partial").unwrap();
        assert_eq!(engine_status(&t), LocalModelStatus::Ready);
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::Ready);

        t.packs.delete(PackKey::Voice("0".into())).await.unwrap();
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::NotInstalled);
        assert!(!t.store.vvm_path("0").exists() && t.store.part_files("0.vvm").is_empty());
        assert_eq!(engine_status(&t), LocalModelStatus::Ready, "声を消してもエンジンは残る");

        t.packs.delete(PackKey::Engine).await.unwrap();
        assert_eq!(engine_status(&t), LocalModelStatus::NotInstalled);
        assert!(!t.store.engine_dir().exists());
        t.packs.delete(PackKey::Engine).await.unwrap();
    }

    #[tokio::test]
    async fn the_snapshot_lists_every_voice_pack_with_its_characters_credits_and_size() {
        let t = build(engine_assets_for("linux", "x86_64"), None);
        let snap = t.packs.snapshot();
        assert!(snap.supported);
        assert_eq!(snap.terms_version, terms_version());
        assert!(!snap.terms_accepted, "同意は呼び出し側が埋める");
        assert_eq!(snap.engine.size_bytes, 1_457_717 + 6_765_720 + 23_646_843);
        assert_eq!(snap.voices.len(), 27);
        let v0 = &snap.voices[0];
        assert_eq!(v0.id, "0");
        assert_eq!(v0.pack.size_bytes, 59_308_488);
        assert_eq!(v0.characters[0].name, "四国めたん");
        assert_eq!(v0.characters[0].credit, "VOICEVOX:四国めたん");
        assert_eq!(v0.characters[0].styles[0], StyleInfo { id: "2".into(), name: "ノーマル".into() });
    }
}
```

- [ ] **Step 2: テストが失敗することを確かめる**

`mod.rs` に `pub mod packs;` を足したうえで、Run: `cd src-tauri && cargo test --lib tts::voicevox::packs 2>&1 | tail -8`
Expected: `cannot find type 'Packs'` などのコンパイルエラー。

- [ ] **Step 3: 実装する**

`packs.rs` の先頭:

```rust
//! エンジン一式・声(VVM)の取得・削除・状態(Issue #625 段階2)。同時に走る取得は 1 つ。
//! 進捗は `Notify` で知らせる(Tauri の Event への変換は `commands/speech_core.rs`。ここは Tauri に依存しない)。
//! 取得は `translate/download.rs` の `download()`(Range 再開・SHA-256 検証・キャンセル)を使う。

use super::archive::{extract_tgz, extract_zip, ExtractError, Limits, Select};
use super::catalog::{catalog, terms_version, vvm_url, Asset, EngineAssets};
use super::engine::CoreEngine;
use super::store::Store;
use crate::error::{Error, Result};
use crate::translate::download::{download, DownloadError, ModelSpec, Phase};
use crate::translate::model_store::{available_space, LocalModelStatus};
use crate::tts::{invalid, ERR_UNSUPPORTED};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 状態が変わったことを知らせる。呼ぶ側が `Packs::snapshot` を引き直す。
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// 進捗の通知の最小間隔(検証中と終了状態は間引かない)。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// 展開後のサイズの上限。公式の取得物(CORE 4〜6 MB、ONNX Runtime 12〜18 MB、辞書 約 103 MB)に対して、余裕を持たせる。
const CORE_LIMITS: Limits = Limits { max_file: 64 << 20, max_total: 64 << 20 };
const ORT_LIMITS: Limits = Limits { max_file: 64 << 20, max_total: 64 << 20 };
const DICT_LIMITS: Limits = Limits { max_file: 160 << 20, max_total: 192 << 20 };

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackKey {
    Engine,
    Voice(String),
}

impl PackKey {
    /// コマンドの引数から。`"engine"` か、カタログにある VVM の ID だけを受け付ける。
    pub fn parse(s: &str) -> Result<Self> {
        if s == "engine" {
            Ok(Self::Engine)
        } else if catalog().vvm(s).is_some() {
            Ok(Self::Voice(s.to_string()))
        } else {
            Err(Error::Invalid("不明な取得対象です".into()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub status: LocalModelStatus,
    #[specta(type = specta_typescript::Number)]
    pub size_bytes: u64,
    /// 途中まで取得済みで、再開できるファイルのバイト数(無ければ 0)。
    #[specta(type = specta_typescript::Number)]
    pub partial_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleInfo {
    /// スタイル ID の数値の文字列(声の ID と同じ)。
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CharacterInfo {
    pub name: String,
    /// クレジット表記(例 `VOICEVOX:ずんだもん`)。
    pub credit: String,
    pub styles: Vec<StyleInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoicePackInfo {
    pub id: String,
    pub pack: PackInfo,
    pub characters: Vec<CharacterInfo>,
}

/// 設定画面に出す、内蔵エンジンの状態の全体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCoreInfo {
    /// この環境が内蔵エンジンに対応しているか。
    pub supported: bool,
    /// 現行の規約の版(`start_speech_core_download` の引数に渡す)。
    pub terms_version: String,
    /// 現行の版に同意済みか。
    pub terms_accepted: bool,
    pub engine: PackInfo,
    pub voices: Vec<VoicePackInfo>,
}

enum PackError {
    Download(DownloadError),
    Extract(ExtractError),
}

impl PackError {
    fn reason(&self) -> String {
        match self {
            Self::Download(e) => e.reason(),
            Self::Extract(e) => e.reason(),
        }
    }

    fn log_tag(&self) -> String {
        match self {
            Self::Download(e) => e.log_tag(),
            Self::Extract(e) => e.log_tag().to_string(),
        }
    }
}

fn io_dl(e: std::io::Error) -> PackError {
    PackError::Download(DownloadError::Io(e.to_string()))
}

#[derive(Clone, Copy)]
enum Kind {
    Core,
    Ort,
    Dict,
}

struct Inner {
    /// 実行中の取得のキャンセルフラグ。`Some` の間は、新しい開始・削除を拒否する。
    running: Option<Arc<AtomicBool>>,
    /// 取得中・検証中・失敗の状態。無ければディスクの状態(Ready / NotInstalled)。
    transient: HashMap<PackKey, LocalModelStatus>,
    /// 削除中(アンロードの待ち〜ファイルの削除)。この間は、開始も 2 つ目の削除も拒否する。
    deleting: bool,
}

pub struct Packs {
    store: Arc<Store>,
    /// `None` は、対応しない環境。
    assets: Option<EngineAssets>,
    http: reqwest::Client,
    engine: CoreEngine,
    vvm_specs: HashMap<String, ModelSpec>,
    inner: Mutex<Inner>,
}

impl Packs {
    pub fn new(store: Arc<Store>, assets: Option<EngineAssets>, http: reqwest::Client, engine: CoreEngine) -> Self {
        let vvm_specs = catalog()
            .vvms
            .iter()
            .map(|v| (v.id.clone(), ModelSpec { url: vvm_url(&v.id), size: v.size, sha256: v.sha256.clone() }))
            .collect();
        Self { store, assets, http, engine, vvm_specs, inner: Mutex::new(Inner { running: None, transient: HashMap::new(), deleting: false }) }
    }

    #[cfg(test)]
    pub(crate) fn override_voice_spec(&mut self, id: &str, spec: ModelSpec) {
        self.vvm_specs.insert(id.to_string(), spec);
    }

    pub(crate) fn is_running(&self) -> bool {
        self.inner.lock().unwrap().running.is_some()
    }

    fn size_of(&self, key: &PackKey) -> u64 {
        match key {
            PackKey::Engine => self.assets.as_ref().map_or(0, |a| a.core.size + a.ort.size + a.dict.size),
            PackKey::Voice(id) => self.vvm_specs.get(id).map_or(0, |s| s.size),
        }
    }

    fn voice_file(id: &str) -> String {
        format!("{id}.vvm")
    }

    fn is_ready(&self, key: &PackKey) -> bool {
        match key {
            PackKey::Engine => self.assets.as_ref().is_some_and(|a| self.store.engine_ready(a)),
            PackKey::Voice(id) => self
                .vvm_specs
                .get(id)
                .is_some_and(|s| std::fs::metadata(self.store.vvm_path(id)).is_ok_and(|m| m.is_file() && m.len() == s.size)),
        }
    }

    fn pack_info(&self, key: &PackKey, g: &Inner) -> PackInfo {
        let status = match g.transient.get(key) {
            Some(t) => t.clone(),
            None if self.is_ready(key) => LocalModelStatus::Ready,
            None => LocalModelStatus::NotInstalled,
        };
        let partial_bytes = match key {
            PackKey::Engine => 0,
            PackKey::Voice(id) => self.vvm_specs.get(id).map_or(0, |s| self.store.partial_bytes(&Self::voice_file(id), &s.sha256)),
        };
        PackInfo { status, size_bytes: self.size_of(key), partial_bytes }
    }

    /// 設定画面に出す状態の全体。`terms_accepted` は設定(`SettingsData`)にあるので、呼び出し側が埋める。
    pub fn snapshot(&self) -> SpeechCoreInfo {
        let g = self.inner.lock().unwrap();
        let voices = catalog()
            .vvms
            .iter()
            .map(|v| VoicePackInfo {
                id: v.id.clone(),
                pack: self.pack_info(&PackKey::Voice(v.id.clone()), &g),
                characters: v
                    .characters
                    .iter()
                    .map(|c| CharacterInfo {
                        name: c.name.clone(),
                        credit: c.credit.clone(),
                        styles: c.styles.iter().map(|s| StyleInfo { id: s.id.to_string(), name: s.name.clone() }).collect(),
                    })
                    .collect(),
            })
            .collect();
        SpeechCoreInfo {
            supported: self.assets.is_some(),
            terms_version: terms_version(),
            terms_accepted: false,
            engine: self.pack_info(&PackKey::Engine, &g),
            voices,
        }
    }

    /// 取得を始める。同時に走るのは 1 つだけ。取得済みは拒否する。進捗は `notify` で知らせる。
    pub fn start(self: &Arc<Self>, key: PackKey, notify: Notify) -> Result<()> {
        if key == PackKey::Engine && self.assets.is_none() {
            return Err(invalid(ERR_UNSUPPORTED));
        }
        if let PackKey::Voice(id) = &key {
            if !self.vvm_specs.contains_key(id) {
                return Err(Error::Invalid("不明な取得対象です".into()));
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let total = self.size_of(&key);
        {
            let mut g = self.inner.lock().unwrap();
            if g.running.is_some() {
                return Err(Error::Invalid("すでにダウンロード中です".into()));
            }
            if g.deleting {
                return Err(Error::Invalid("削除中です。完了してからやり直してください".into()));
            }
            if self.is_ready(&key) {
                return Err(Error::Invalid("取得済みです".into()));
            }
            g.running = Some(cancel.clone());
            g.transient.insert(key.clone(), LocalModelStatus::Downloading { downloaded: 0, total });
        }
        notify();
        let this = self.clone();
        // `tauri::async_runtime::spawn` ではなく `tokio::spawn`: この型は Tauri に依存させない(`LocalModels` と同じ)。
        tokio::spawn(async move { this.run(key, cancel, notify).await });
        Ok(())
    }

    /// 実行中の取得を止める。途中までのファイルは残り、次回は続きから再開する。
    pub fn cancel(&self) {
        if let Some(c) = &self.inner.lock().unwrap().running {
            c.store(true, Ordering::SeqCst);
        }
    }

    async fn run(self: Arc<Self>, key: PackKey, cancel: Arc<AtomicBool>, notify: Notify) {
        let r = match &key {
            PackKey::Engine => self.run_engine(&cancel, &notify).await,
            PackKey::Voice(id) => self.run_voice(id, &cancel, &notify).await,
        };
        self.finish(&key, r, &notify);
    }

    fn finish(&self, key: &PackKey, r: std::result::Result<(), PackError>, notify: &Notify) {
        {
            let mut g = self.inner.lock().unwrap();
            g.running = None;
            match r {
                Ok(()) | Err(PackError::Download(DownloadError::Cancelled)) => {
                    // キャンセルは失敗ではない。.part は残る(再開できる)。ディスクの状態に戻す。
                    g.transient.remove(key);
                }
                Err(e) => {
                    // 失敗の種類だけをログに残す(URL・パスは含めない)。
                    log::warn!("speech core pack failed: {}", e.log_tag());
                    g.transient.insert(key.clone(), LocalModelStatus::Failed { reason: e.reason() });
                }
            }
        }
        notify();
    }

    fn set_status(&self, key: &PackKey, status: LocalModelStatus, notify: &Notify) {
        self.inner.lock().unwrap().transient.insert(key.clone(), status);
        notify();
    }

    /// 1 つのファイルを取得して検証する。進捗は、`base` バイト取得済みの続きとして `total` に対して出す。
    #[allow(clippy::too_many_arguments)]
    async fn fetch(&self, key: &PackKey, spec: &ModelSpec, part: &Path, dest: &Path, base: u64, total: u64, cancel: &AtomicBool, notify: &Notify) -> std::result::Result<(), PackError> {
        let mut last = Instant::now() - PROGRESS_INTERVAL;
        download(&self.http, spec, part, dest, cancel, &available_space, &mut |p| {
            let status = match p {
                Phase::Downloading { downloaded, .. } => LocalModelStatus::Downloading { downloaded: base + downloaded, total },
                Phase::Verifying => LocalModelStatus::Verifying,
            };
            // 状態は常に更新し、通知だけ間引く(検証中は必ず通知する)。
            self.inner.lock().unwrap().transient.insert(key.clone(), status.clone());
            if matches!(status, LocalModelStatus::Verifying) || last.elapsed() >= PROGRESS_INTERVAL {
                last = Instant::now();
                notify();
            }
        })
        .await
        .map_err(PackError::Download)
    }

    async fn run_voice(&self, id: &str, cancel: &AtomicBool, notify: &Notify) -> std::result::Result<(), PackError> {
        let spec = self.vvm_specs.get(id).cloned().ok_or(PackError::Download(DownloadError::BadResponse))?;
        std::fs::create_dir_all(self.store.vvm_dir()).map_err(io_dl)?;
        std::fs::create_dir_all(self.store.downloads_dir()).map_err(io_dl)?;
        let part = self.store.part_path(&Self::voice_file(id), &spec.sha256);
        let dest = self.store.vvm_path(id);
        self.fetch(&PackKey::Voice(id.to_string()), &spec, &part, &dest, 0, spec.size, cancel, notify).await
    }

    async fn run_engine(&self, cancel: &AtomicBool, notify: &Notify) -> std::result::Result<(), PackError> {
        let a = self.assets.clone().ok_or(PackError::Download(DownloadError::BadResponse))?;
        let total = a.core.size + a.ort.size + a.dict.size;
        let tmp = self.store.engine_tmp_dir();
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(self.store.downloads_dir()).map_err(io_dl)?;
        let r = self.engine_steps(&a, total, &tmp, cancel, notify).await.and_then(|()| self.commit_engine(&tmp));
        if r.is_err() {
            // 途中のディレクトリは残さない(`.part` は残す。再開できる)。
            let _ = std::fs::remove_dir_all(&tmp);
        }
        r
    }

    /// 3 つのアーカイブを順に取得して、`tmp` へ展開する。
    async fn engine_steps(&self, a: &EngineAssets, total: u64, tmp: &Path, cancel: &AtomicBool, notify: &Notify) -> std::result::Result<(), PackError> {
        let steps: [(&Asset, &str, Kind); 3] = [(&a.core, "core.zip", Kind::Core), (&a.ort, "ort.tgz", Kind::Ort), (&a.dict, "dict.tgz", Kind::Dict)];
        let mut base = 0u64;
        for (asset, name, kind) in steps {
            let spec = ModelSpec { url: asset.url.clone(), size: asset.size, sha256: asset.sha256.clone() };
            let part = self.store.part_path(name, &asset.sha256);
            let archive = self.store.downloads_dir().join(name);
            self.fetch(&PackKey::Engine, &spec, &part, &archive, base, total, cancel, notify).await?;
            self.set_status(&PackKey::Engine, LocalModelStatus::Verifying, notify); // 展開中
            let (archive_c, tmp_c) = (archive.clone(), tmp.to_path_buf());
            let (core_lib, ort_lib) = (a.core_lib, a.ort_lib);
            let r = tokio::task::spawn_blocking(move || match kind {
                Kind::Core => {
                    let entry = format!("lib/{core_lib}");
                    extract_zip(&archive_c, &tmp_c.join("core"), Select::Files(&[(entry.as_str(), core_lib)]), CORE_LIMITS)
                }
                Kind::Ort => {
                    let entry = format!("lib/{ort_lib}");
                    extract_tgz(&archive_c, &tmp_c.join("ort"), Select::Files(&[(entry.as_str(), ort_lib)]), ORT_LIMITS)
                }
                Kind::Dict => extract_tgz(&archive_c, &tmp_c.join("dict"), Select::All, DICT_LIMITS),
            })
            .await
            .map_err(|e| PackError::Extract(ExtractError::Io(e.to_string())))?;
            let _ = std::fs::remove_file(&archive);
            r.map_err(PackError::Extract)?;
            base += asset.size;
        }
        Ok(())
    }

    /// 展開が全部済んだ `tmp` に READY を置き、本来の場所へリネームする(これが完了の瞬間)。
    fn commit_engine(&self, tmp: &Path) -> std::result::Result<(), PackError> {
        Store::write_ready(tmp).map_err(io_dl)?;
        let dir = self.store.engine_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::rename(tmp, &dir).map_err(io_dl)
    }

    /// 取得済みのものと、途中までのファイルを削除する。ロード済みなら先に解放する。
    /// 取得中と、別の削除の最中は拒否する(削除中は `start` も拒否される)。
    pub async fn delete(&self, key: PackKey) -> Result<()> {
        {
            let mut g = self.inner.lock().unwrap();
            if g.running.is_some() {
                return Err(Error::Invalid("ダウンロード中は削除できません".into()));
            }
            if g.deleting {
                return Err(Error::Invalid("削除中です。完了してからやり直してください".into()));
            }
            g.deleting = true;
        }
        self.engine.unload();
        let r = self.delete_files(&key);
        let mut g = self.inner.lock().unwrap();
        g.deleting = false;
        if r.is_ok() {
            g.transient.remove(&key);
        }
        r
    }

    fn delete_files(&self, key: &PackKey) -> Result<()> {
        let mut targets = vec![];
        let mut dirs = vec![];
        match key {
            PackKey::Engine => {
                dirs.extend([self.store.engine_dir(), self.store.engine_tmp_dir()]);
                for name in ["core.zip", "ort.tgz", "dict.tgz"] {
                    targets.push(self.store.downloads_dir().join(name));
                    targets.extend(self.store.part_files(name));
                }
            }
            PackKey::Voice(id) => {
                targets.push(self.store.vvm_path(id));
                targets.extend(self.store.part_files(&Self::voice_file(id)));
            }
        }
        let fail = |kind: std::io::ErrorKind| {
            log::warn!("speech core files could not be deleted: {kind:?}");
            Error::Invalid("ファイルを削除できませんでした。アプリを再起動してから、もう一度お試しください".into())
        };
        for d in dirs {
            match std::fs::remove_dir_all(&d) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(fail(e.kind())),
            }
        }
        for p in targets {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(fail(e.kind())),
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox 2>&1 | tail -10`
Expected: packs の 10 件を含め、voicevox の全テストが通る。`the_engine_pack_downloads_…` が `.part` や archive の残りで落ちたら、`download()` が成功時に `.part` を `dest` へリネームすること(`translate/download.rs` の末尾)と、`engine_steps` の `remove_file(&archive)` を確かめる。

- [ ] **Step 5: コミットする**

```sh
git add src-tauri/src/tts/voicevox
git commit -m "feat(tts): 内蔵エンジンと声の取得・削除・進捗の状態機械を追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: 設定の型(`engine` に内蔵を足す・規約の同意版を保存する)

**Files:**
- Modify: `src-tauri/src/domain/speech.rs`
- Modify: `src-tauri/src/store/settings.rs`
- Modify: `src-tauri/src/store/settings_transfer.rs`

**Interfaces:**
- Produces: `SpeechEngineKind::VoicevoxCore`(`"voicevoxCore"`)、`SettingsStore::speech_core_terms_version(&self) -> Option<String>`、`SettingsStore::set_speech_core_terms_version(&self, version: &str) -> Result<()>`、`SettingsData.speech_core_terms_version: Option<String>`(`#[serde(default)]`)

- [ ] **Step 1: 失敗するテストを書く**

`domain/speech.rs` の `mod tests` の末尾に足す:

```rust
    #[test]
    fn the_engine_kind_round_trips_as_camel_case() {
        assert_eq!(serde_json::to_string(&SpeechEngineKind::VoicevoxCore).unwrap(), "\"voicevoxCore\"");
        let s: SpeechSettings = serde_json::from_str(r#"{"engine":"voicevoxCore"}"#).unwrap();
        assert_eq!(s.engine, SpeechEngineKind::VoicevoxCore);
        assert!(s.validate().is_ok(), "内蔵は URL が空でも有効");
    }

    #[test]
    fn an_unknown_engine_kind_falls_back_to_external_instead_of_failing_the_whole_file() {
        // 将来のエンジンや、設定ファイルの手編集で、知らない値が入っていても、設定ファイル全体を読めなくしない。
        let s: SpeechSettings = serde_json::from_str(r#"{"enabled":true,"engine":"someFutureEngine"}"#).unwrap();
        assert_eq!(s.engine, SpeechEngineKind::External);
        assert!(s.enabled, "ほかの項目は読める");
    }
```

`store/settings.rs` の `mod tests` の `set_local_model_license_version_is_kept` の次に足す:

```rust
    #[test]
    fn speech_core_terms_version_is_none_until_set_and_then_kept() {
        let d: SettingsData = serde_json::from_str("{}").unwrap();
        assert_eq!(d.speech_core_terms_version, None, "旧い設定ファイルは未同意として読む");
        let s = SettingsStore::new_in_memory();
        assert_eq!(s.speech_core_terms_version(), None);
        s.set_speech_core_terms_version("abc").unwrap();
        assert_eq!(s.speech_core_terms_version().as_deref(), Some("abc"));
    }
```

`store/settings_transfer.rs` の `export_does_not_include_the_local_model_license_version` の次に足す(同じファイルの `current_with_local_account` / `file_with` / `empty_data` / `settings_data` を使う):

```rust
    #[test]
    fn import_keeps_the_current_speech_core_terms_version_and_never_grants_consent() {
        let mut current = current_with_local_account();
        current.speech_core_terms_version = Some("v1".into());
        let plan = plan_import(&current, file_with(empty_data())).unwrap();
        assert_eq!(plan.data.speech_core_terms_version.as_deref(), Some("v1"));

        let current = current_with_local_account();
        let plan = plan_import(&current, file_with(empty_data())).unwrap();
        assert_eq!(plan.data.speech_core_terms_version, None, "インポートで規約に同意済みになってはいけない");
    }

    #[test]
    fn export_does_not_include_the_speech_core_terms_version_or_the_engine_choice() {
        let mut d = settings_data();
        d.speech_core_terms_version = Some("v1".into());
        d.speech.engine = crate::domain::SpeechEngineKind::VoicevoxCore;
        let json = serde_json::to_string(&build_export(&d, "1.2.3", "2026-11-20T00:00:00Z")).unwrap().to_lowercase();
        assert!(!json.contains("speech_core") && !json.contains("speechcore") && !json.contains("voicevoxcore"), "{json}");
    }
```

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cd src-tauri && cargo test --lib domain::speech store::settings 2>&1 | tail -10`
Expected: `no variant named 'VoicevoxCore'` / `no field 'speech_core_terms_version'` のコンパイルエラー。

- [ ] **Step 3: 実装する**

`domain/speech.rs`: 列挙体の doc と、`engine` の読み込みを直す。`SpeechEngineKind` を次のとおりにする:

```rust
/// 読み上げに使うエンジンの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SpeechEngineKind {
    /// 外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)に HTTP で接続する。
    #[default]
    External,
    /// 内蔵の VOICEVOX CORE(デスクトップのみ。エンジン一式と声は、設定画面から取得する)。Issue #625 段階2。
    VoicevoxCore,
}

/// 知らないエンジンの種類(将来のエンジン・手編集)は `External` として読む。設定ファイル全体が読めなくなるのを避ける。
fn engine_or_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<SpeechEngineKind, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}
```

`SpeechSettings` の `engine` フィールドの属性を `#[serde(default)]` から変える:

```rust
    #[serde(default, deserialize_with = "engine_or_default")]
    pub engine: SpeechEngineKind,
```

`SpeechSettings` の `#[serde(default, deserialize_with = "engine_or_default")]` を、specta の derive が受け付けない(コンパイルエラー、または TS の型が変わる)場合は、属性を `#[serde(default)]` に戻し、`SpeechEngineKind` の `Deserialize` を手で実装する(`#[derive(Deserialize)]` を外し、`engine_or_default` を削除する):

```rust
impl<'de> Deserialize<'de> for SpeechEngineKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "voicevoxCore" => Self::VoicevoxCore,
            _ => Self::External, // 知らない値(将来のエンジン・手編集)は、外部として読む。
        })
    }
}
```
`generates_frontend_bindings` で、TS の型が `"external" | "voicevoxCore"` のままなことを確かめる(Task 9 Step 8)。

`store/settings.rs`: `SettingsData` の `speech` フィールド(`pub(crate) speech: crate::domain::SpeechSettings,`)の次に足す:

```rust
    /// 内蔵 VOICEVOX CORE の規約(VOICEVOX 音声モデル・ONNX Runtime)に同意した版(Issue #625 段階2)。
    /// **エクスポートには含めない**。インポートでは現在の値を維持する(取り込みで同意済みにならない)。
    #[serde(default)]
    pub(crate) speech_core_terms_version: Option<String>,
```

`set_local_model_license_version` の次(同じ `impl SettingsStore` の中)に足す:

```rust
    pub fn speech_core_terms_version(&self) -> Option<String> {
        self.data.lock().unwrap().speech_core_terms_version.clone()
    }

    pub fn set_speech_core_terms_version(&self, version: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.speech_core_terms_version = Some(version.to_string());
        self.save(&guard)
    }
```

`SettingsData` を全フィールド指定で構築している箇所(`local_model_license_version: None,` と `speech: Default::default(),` が並ぶ所)に `speech_core_terms_version: None,` を足す。コンパイルエラーが指す箇所を、すべて直す(`grep -n "local_model_license_version: None" src-tauri/src -r`)。

`store/settings_transfer.rs`: `plan_import` の、`local_model_license_version: current.local_model_license_version.clone(),` の次の行に足す:

```rust
        speech_core_terms_version: current.speech_core_terms_version.clone(),
```
(ファイル内に `SettingsData { … }` を全フィールドで構築する箇所があれば、コンパイラの指摘に従って同様に足す。エクスポート(`build_export`)には何も足さない。)

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cd src-tauri && cargo test --lib domain::speech store:: 2>&1 | tail -10`
Expected: 全部通る(新規 5 件を含む)。

- [ ] **Step 5: コミットする**

```sh
git add src-tauri/src
git commit -m "feat(tts): 読み上げのエンジンに内蔵を足し、規約の同意版を保存する(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: 配線・コマンド・イベント(Rust の仕上げ)

**Files:**
- Modify: `src-tauri/src/tts/voicevox/mod.rs`(`SpeechCore`、`native_factory`)
- Modify: `src-tauri/src/state.rs`、`src-tauri/src/events.rs`、`src-tauri/src/tts/runtime.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`
- Create: `src-tauri/src/commands/speech_core.rs`
- Modify(生成): `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: `Packs`・`CoreEngine`・`Store`・`catalog::{terms, terms_version}`・`SettingsStore::{speech_core_terms_version, set_speech_core_terms_version}`
- Produces: `struct SpeechCore { pub store: Arc<Store>, pub engine: CoreEngine, pub packs: Arc<Packs> }`(`new(root: PathBuf, http: reqwest::Client) -> Self`、`unconfigured() -> Self`)、`AppState.speech_core: Arc<SpeechCore>`・`AppState::with_speech_core(self, Arc<SpeechCore>) -> Self`、Event `SpeechCoreStatusChanged { info: SpeechCoreInfo }`、コマンド `get_speech_core_info() -> SpeechCoreInfo`、`get_speech_core_terms() -> SpeechCoreTerms { version, voiceModels, onnxruntime }`、`start_speech_core_download(pack: String, acceptedTermsVersion: String)`、`cancel_speech_core_download()`、`delete_speech_core_pack(pack: String)`、`list_speech_core_voices() -> Vec<Voice>`

- [ ] **Step 1: `SpeechCore` を書く**

`tts/voicevox/mod.rs` を、次の内容にする(Task 2〜7 で足した `pub mod` 行をまとめる):

```rust
//! 内蔵 VOICEVOX CORE(Issue #625 段階2)。設計は `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`。
//! ネイティブの呼び出し(`ffi::NativeCore`)だけ `cfg(desktop)`。ほかは全環境でビルドし、Android などでは `supported = false`。

pub mod archive;
pub mod catalog;
pub mod engine;
pub mod ffi;
pub mod packs;
pub mod store;
#[cfg(test)]
pub(crate) mod test_util;

use catalog::EngineAssets;
use engine::{BackendFactory, CoreEngine};
use ffi::{CoreBackend, CoreError};
use packs::Packs;
use std::path::PathBuf;
use std::sync::Arc;
use store::Store;

/// 内蔵エンジンの、置き場・エンジン本体・取得の状態機械をまとめたもの。`AppState.speech_core`。
pub struct SpeechCore {
    pub store: Arc<Store>,
    pub engine: CoreEngine,
    pub packs: Arc<Packs>,
}

impl SpeechCore {
    /// `root` は `app_data_dir/voicevox`。`http` は、固定の GitHub の URL だけに接続するダウンロード用のクライアント。
    pub fn new(root: PathBuf, http: reqwest::Client) -> Self {
        let assets = catalog::engine_assets();
        let store = Arc::new(Store::new(root));
        let engine = CoreEngine::new(store.clone(), assets.clone(), native_factory(store.clone(), assets.clone()));
        let packs = Arc::new(Packs::new(store.clone(), assets, http, engine.clone()));
        Self { store, engine, packs }
    }

    /// 非対応(テスト・`AppState::new` の既定)。何もディスクに書かず、取得もできない。
    pub fn unconfigured() -> Self {
        let store = Arc::new(Store::new(std::env::temp_dir().join("tsumugi-voicevox-unconfigured")));
        let factory: BackendFactory = Arc::new(|| Err(CoreError::Load("unconfigured".into())));
        let engine = CoreEngine::new(store.clone(), None, factory);
        let packs = Arc::new(Packs::new(store.clone(), None, reqwest::Client::new(), engine.clone()));
        Self { store, engine, packs }
    }
}

#[cfg(desktop)]
fn native_factory(store: Arc<Store>, assets: Option<EngineAssets>) -> BackendFactory {
    Arc::new(move || {
        let a = assets.as_ref().ok_or_else(|| CoreError::Load("unsupported platform".into()))?;
        let core = ffi::NativeCore::load(&store.core_lib(a), &store.ort_lib(a), &store.dict_dir(), ffi::default_cpu_threads())?;
        Ok(Arc::new(core) as Arc<dyn CoreBackend>)
    })
}

#[cfg(not(desktop))]
fn native_factory(_store: Arc<Store>, _assets: Option<EngineAssets>) -> BackendFactory {
    Arc::new(|| Err(CoreError::Load("unsupported platform".into())))
}
```

(`CoreBackend` は `cfg(not(desktop))` では未使用になり警告が出る。`#[cfg_attr(not(desktop), allow(unused_imports))]` を `use ffi::{CoreBackend, CoreError};` の上に付ける。)

- [ ] **Step 2: `AppState`・イベント・ワーカーの配線**

`state.rs`:
  - `pub speech: std::sync::Arc<crate::tts::manager::SpeechManager>,` の次に足す:
    ```rust
        /// 内蔵 VOICEVOX CORE(Issue #625 段階2)。既定は非対応(`unconfigured`)で、起動時に `with_speech_core` で差し替える。
        pub speech_core: std::sync::Arc<crate::tts::voicevox::SpeechCore>,
    ```
  - 構築箇所(`speech: std::sync::Arc::new(crate::tts::manager::SpeechManager::new()),` の次)に足す: `speech_core: std::sync::Arc::new(crate::tts::voicevox::SpeechCore::unconfigured()),`
  - `with_local_models` の次に足す:
    ```rust
        pub fn with_speech_core(mut self, speech_core: std::sync::Arc<crate::tts::voicevox::SpeechCore>) -> Self {
            self.speech_core = speech_core;
            self
        }
    ```

`events.rs`(`SpeechStatusChanged` の次):

```rust
/// 内蔵 VOICEVOX CORE のエンジン一式・声(VVM)の状態が変わった(Issue #625 段階2)。
/// 取得の進捗・完了・失敗・削除のたびに、状態の全体(スナップショット)を送る。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCoreStatusChanged {
    pub info: crate::tts::voicevox::packs::SpeechCoreInfo,
}
```

`tts/runtime.rs` の `engine()` の先頭に足す(`use crate::domain::{SpeechEngineKind, SpeechSettings};` に直す。`SpeechEngineKind` が `crate::domain` から出ていなければ `crate::domain::speech::SpeechEngineKind`):

```rust
        // 内蔵エンジンは、`AppState` が持つ 1 つを使い回す(ロード済みのモデルを共有する)。未取得・非対応のときは、
        // `synthesize` が `ERR_NOT_READY` / `ERR_UNSUPPORTED` を返し、失敗の連続で `Error` 状態になる。
        if settings.engine == SpeechEngineKind::VoicevoxCore {
            return Ok(Arc::new(self.app.state::<AppState>().speech_core.engine.clone()));
        }
```

- [ ] **Step 3: 失敗するテストを書く(コマンド)**

`commands/speech_core.rs` を、まずテストだけで作る(実装は Step 5):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SettingsStore;
    use crate::translate::download::ModelSpec;
    use crate::tts::voicevox::engine::CoreEngine;
    use crate::tts::voicevox::ffi::CoreError;
    use crate::tts::voicevox::packs::Packs;
    use crate::tts::voicevox::store::Store;
    use crate::tts::voicevox::test_util::{sha256_hex, temp_root};
    use crate::tts::voicevox::SpeechCore;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn noop() -> Notify {
        Arc::new(|| {})
    }

    fn unconfigured() -> AppState {
        AppState::new_for_test(SettingsStore::new_in_memory())
    }

    /// 声 `0` の取得先を、偽のサーバーに向けた状態。
    fn with_server(server: &MockServer, body: &[u8]) -> AppState {
        let store = Arc::new(Store::new(temp_root()));
        let engine = CoreEngine::new(store.clone(), None, Arc::new(|| Err(CoreError::Load("n/a".into()))));
        let mut packs = Packs::new(store.clone(), None, reqwest::Client::new(), engine.clone());
        packs.override_voice_spec("0", ModelSpec { url: format!("{}/0.vvm", server.uri()), size: body.len() as u64, sha256: sha256_hex(body) });
        let core = SpeechCore { store, engine, packs: Arc::new(packs) };
        unconfigured().with_speech_core(Arc::new(core))
    }

    async fn wait_idle(s: &AppState) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while s.speech_core.packs.is_running() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("取得が終わらない");
    }

    #[tokio::test]
    async fn a_wrong_or_empty_terms_version_is_rejected_and_nothing_is_recorded() {
        let s = unconfigured();
        for v in ["", "1999-01-01", "  ", &terms_version().to_uppercase()] {
            let r = start_core(&s, "0", v, noop());
            assert!(matches!(r, Err(Error::Invalid(ref m)) if m.contains("規約")), "{v:?}: {r:?}");
        }
        assert_eq!(s.settings.speech_core_terms_version(), None);
    }

    #[tokio::test]
    async fn an_unknown_pack_and_an_unsupported_environment_are_rejected_without_recording_consent() {
        let s = unconfigured();
        assert!(start_core(&s, "nope", &terms_version(), noop()).is_err());
        assert!(start_core(&s, "s0", &terms_version(), noop()).is_err(), "歌声だけの VVM は取得できない");
        match start_core(&s, "engine", &terms_version(), noop()) {
            Err(Error::Invalid(m)) => assert_eq!(m, crate::tts::ERR_UNSUPPORTED),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.settings.speech_core_terms_version(), None, "開始できなかったら同意を記録しない");
    }

    #[tokio::test]
    async fn consent_is_recorded_after_a_successful_start_and_shown_in_the_info() {
        let server = MockServer::start().await;
        let body = vec![7u8; 2048];
        Mock::given(method("GET")).and(path("/0.vvm")).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone())).mount(&server).await;
        let s = with_server(&server, &body);
        assert!(!info(&s).terms_accepted);

        start_core(&s, "0", &terms_version(), noop()).unwrap();
        wait_idle(&s).await;

        assert_eq!(s.settings.speech_core_terms_version().as_deref(), Some(terms_version().as_str()));
        let i = info(&s);
        assert!(i.terms_accepted);
        assert_eq!(i.voices[0].pack.status, crate::translate::model_store::LocalModelStatus::Ready);
    }

    #[test]
    fn an_older_consent_is_not_accepted_for_the_current_terms() {
        let s = unconfigured();
        s.settings.set_speech_core_terms_version("older-version").unwrap();
        assert!(!info(&s).terms_accepted, "規約が変わったら、再同意が要る");
    }

    #[test]
    fn the_terms_command_returns_both_texts_and_the_current_version() {
        let t = terms_core();
        assert_eq!(t.version, terms_version());
        assert!(t.voice_models.contains("音声モデル") && t.onnxruntime.contains("ONNX Runtime"));
    }

    #[tokio::test]
    async fn the_unconfigured_engine_reports_unsupported_and_lists_no_voices() {
        let s = unconfigured();
        assert!(!info(&s).supported);
        assert!(list_voices_core(&s).await.unwrap().is_empty());
        let r = s.speech_core.engine.synthesize("a", "3", &crate::tts::engine::SynthParams { speed_scale: 1.0, volume: 1.0 }).await;
        assert!(matches!(r, Err(Error::Invalid(ref m)) if m == crate::tts::ERR_UNSUPPORTED), "{r:?}");
    }

    #[test]
    fn the_engine_can_be_switched_to_the_built_in_core_with_no_url() {
        let s = unconfigured();
        let saved = crate::commands::speech::set_settings_core(
            &s,
            crate::domain::SpeechSettings { enabled: true, engine: crate::domain::SpeechEngineKind::VoicevoxCore, ..Default::default() },
        )
        .unwrap();
        assert_eq!(saved.engine, crate::domain::SpeechEngineKind::VoicevoxCore);
        assert_eq!(s.settings.load_speech().engine, crate::domain::SpeechEngineKind::VoicevoxCore);
    }
}
```

- [ ] **Step 4: テストが失敗することを確かめる**

`commands/mod.rs` に `pub mod speech_core;` を足したうえで、Run: `cd src-tauri && cargo test --lib commands::speech_core 2>&1 | tail -8`
Expected: `cannot find function 'start_core'` などのコンパイルエラー。

- [ ] **Step 5: コマンドを実装する**

`commands/speech_core.rs` の先頭(テストの上):

```rust
//! 内蔵 VOICEVOX CORE の取得・状態・削除のコマンド(Issue #625 段階2)。

use crate::error::{Error, Result};
use crate::events::SpeechCoreStatusChanged;
use crate::state::AppState;
use crate::tts::engine::{SpeechEngine, Voice};
use crate::tts::voicevox::catalog::{terms, terms_version};
use crate::tts::voicevox::packs::{Notify, PackKey, SpeechCoreInfo};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event as _;

/// 規約の確認画面に出す 2 つの規約の全文と、現行の版。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCoreTerms {
    pub version: String,
    pub voice_models: String,
    pub onnxruntime: String,
}

pub(crate) fn info(s: &AppState) -> SpeechCoreInfo {
    let mut i = s.speech_core.packs.snapshot();
    i.terms_accepted = s.settings.speech_core_terms_version().as_deref() == Some(i.terms_version.as_str());
    i
}

pub(crate) fn terms_core() -> SpeechCoreTerms {
    let t = terms();
    SpeechCoreTerms { version: terms_version(), voice_models: t.voice_models.replace('\r', ""), onnxruntime: t.onnxruntime.replace('\r', "") }
}

/// 規約への同意を Rust 側で強制してから、取得を始める。同意は、開始できたあとに記録する。
pub(crate) fn start_core(s: &AppState, pack: &str, accepted_terms_version: &str, notify: Notify) -> Result<()> {
    if accepted_terms_version != terms_version() {
        return Err(Error::Invalid("規約への同意が必要です(版が一致しません)".into()));
    }
    let key = PackKey::parse(pack)?;
    s.speech_core.packs.start(key, notify)?;
    s.settings.set_speech_core_terms_version(&terms_version())
}

pub(crate) async fn list_voices_core(s: &AppState) -> Result<Vec<Voice>> {
    s.speech_core.engine.voices().await
}

/// 状態が変わるたびに、スナップショットを Event で送る。
fn notify_for(app: AppHandle) -> Notify {
    Arc::new(move || {
        let state = app.state::<AppState>();
        let _ = SpeechCoreStatusChanged { info: info(&state) }.emit(&app);
    })
}

/// 内蔵エンジンの状態(エンジン一式・声ごと・対応可否・規約の版と同意済みか)。
#[tauri::command]
#[specta::specta]
pub async fn get_speech_core_info(state: State<'_, AppState>) -> Result<SpeechCoreInfo> {
    Ok(info(&state))
}

/// 規約の全文(確認画面用)。
#[tauri::command]
#[specta::specta]
pub async fn get_speech_core_terms() -> Result<SpeechCoreTerms> {
    Ok(terms_core())
}

/// 取得を始める。`pack` は `"engine"` か、声(VVM)の ID。`accepted_terms_version` が現行の版と一致しなければ拒否する。
/// 進捗は Event `SpeechCoreStatusChanged`。設定画面を閉じても続く。
#[tauri::command]
#[specta::specta]
pub async fn start_speech_core_download(app: AppHandle, state: State<'_, AppState>, pack: String, accepted_terms_version: String) -> Result<()> {
    start_core(&state, &pack, &accepted_terms_version, notify_for(app))
}

/// 実行中の取得を止める。途中までのファイルは残り、次回は続きから再開する。
#[tauri::command]
#[specta::specta]
pub async fn cancel_speech_core_download(state: State<'_, AppState>) -> Result<()> {
    state.speech_core.packs.cancel();
    Ok(())
}

/// 取得済みのものと、途中までのファイルを削除する。取得中は拒否する。
#[tauri::command]
#[specta::specta]
pub async fn delete_speech_core_pack(app: AppHandle, state: State<'_, AppState>, pack: String) -> Result<()> {
    state.speech_core.packs.delete(PackKey::parse(&pack)?).await?;
    let _ = SpeechCoreStatusChanged { info: info(&state) }.emit(&app);
    Ok(())
}

/// 取得済みの声(設定画面・カラムの声の選択肢)。
#[tauri::command]
#[specta::specta]
pub async fn list_speech_core_voices(state: State<'_, AppState>) -> Result<Vec<Voice>> {
    list_voices_core(&state).await
}
```

- [ ] **Step 6: テストが通ることを確かめる**

Run: `cd src-tauri && cargo test --lib commands::speech_core 2>&1 | tail -10`
Expected: 7 passed。

- [ ] **Step 7: `lib.rs` に登録し、起動時の配線を足す**

`commands/mod.rs` に `pub mod speech_core;`(Step 4 で足してあれば不要)。

`lib.rs`:
  - コマンドの登録(`commands::speech::get_speech_status,` の次)に足す:
    ```rust
            commands::speech_core::get_speech_core_info,
            commands::speech_core::get_speech_core_terms,
            commands::speech_core::start_speech_core_download,
            commands::speech_core::cancel_speech_core_download,
            commands::speech_core::delete_speech_core_pack,
            commands::speech_core::list_speech_core_voices,
    ```
  - イベントの登録(`events::SpeechStatusChanged,` の次)に足す: `events::SpeechCoreStatusChanged,`
  - 起動時の配線。ローカル翻訳の準備(`let local_models = std::sync::Arc::new(translate::LocalModels::new(models_dir, model_http, local_translate.clone()));`)を、`model_http.clone()` を渡す形に直し、その次に足す:
    ```rust
            // 内蔵 VOICEVOX CORE(Issue #625 段階2)。取得物は app_data_dir/voicevox/ に置く。
            // 古い版・途中のファイルは、ロードより前に掃除する。使われなくなったモデルは、アイドルで解放する。
            let speech_core = std::sync::Arc::new(tts::voicevox::SpeechCore::new(
                app.path().app_data_dir().expect("no app data dir").join("voicevox"),
                model_http.clone(),
            ));
            speech_core.store.clean_stale();
            tauri::async_runtime::spawn(tts::voicevox::engine::run_idle_sweeper(
                speech_core.engine.clone(),
                tts::voicevox::engine::IDLE_UNLOAD,
                tts::voicevox::engine::SWEEP_INTERVAL,
            ));
    ```
  - `AppState::new(...)` のビルダーの末尾(`.with_local_models(local_models),` の `,` の前)に `.with_speech_core(speech_core)` を足す。

- [ ] **Step 8: バインディングを再生成し、全体を確かめる**

Run:
```sh
cd src-tauri && cargo test --lib generates_frontend_bindings 2>&1 | tail -5
grep -c "speechCoreStatusChanged\|getSpeechCoreInfo\|startSpeechCoreDownload\|SpeechCoreInfo\|listSpeechCoreVoices" ../frontend/src/bindings/tauri.gen.ts
grep -n "SpeechEngineKind = " ../frontend/src/bindings/tauri.gen.ts
cargo test --lib 2>&1 | tail -5
cd ../frontend && pnpm check 2>&1 | tail -4
```
Expected: `generates_frontend_bindings` が通る。grep の件数が 1 以上で、`SpeechEngineKind = "external" | "voicevoxCore"`。`cargo test --lib` が全部通る(実物・実機が要るものは `ignored`)。`pnpm check` はエラー 0(まだフロントは変えていないので、型の追加だけ)。

- [ ] **Step 9: コミットする**

```sh
git add src-tauri frontend/src/bindings
git commit -m "feat(tts): 内蔵エンジンの取得・状態・削除のコマンドとイベントを配線する(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: フロントの部品(文面・規約の確認・エンジン一式と声の管理)

**Files:**
- Create: `frontend/src/lib/speechCore.ts`、`frontend/src/lib/speechCore.test.ts`
- Create: `frontend/src/ui/settings/SpeechCoreTermsDialog.svelte`、`frontend/src/ui/settings/SpeechCoreTermsDialog.test.ts`
- Create: `frontend/src/ui/settings/SpeechCoreSection.svelte`、`frontend/src/ui/settings/SpeechCoreSection.test.ts`

**Interfaces:**
- Consumes(生成された型・コマンド・イベント): `SpeechCoreInfo`、`PackInfo`、`VoicePackInfo`、`SpeechCoreTerms`、`Voice`、`commands.getSpeechCoreInfo()`・`getSpeechCoreTerms()`・`startSpeechCoreDownload(pack, acceptedTermsVersion)`・`cancelSpeechCoreDownload()`・`deleteSpeechCorePack(pack)`、`events.speechCoreStatusChanged`(payload `{ info }`)、`lib/localModel.ts` の `formatBytes` / `statusText`
- Produces: `lib/speechCore.ts` の `speechErrorText(message: string): string`、`creditFor(info: SpeechCoreInfo | null | undefined, voiceId: string): string | null`、`installedVoices(info: SpeechCoreInfo | null | undefined): Voice[]`(Rust の `voices()` と同じ並び・同じ label)、`voiceCharactersLabel(v: VoicePackInfo): string`。`SpeechCoreSection.svelte` は `$bindable` の prop `info`(`SpeechCoreInfo | null`)を、親が読めるように公開する。

- [ ] **Step 1: `lib/speechCore.ts` の失敗するテストを書く**

`frontend/src/lib/speechCore.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { SpeechCoreInfo } from "../bindings/tauri.gen";
import { creditFor, installedVoices, speechErrorText, voiceCharactersLabel } from "./speechCore";

const pack = (status: SpeechCoreInfo["engine"]["status"] = { type: "notInstalled" }) => ({ status, sizeBytes: 1000, partialBytes: 0 });

const info = (): SpeechCoreInfo => ({
  supported: true,
  termsVersion: "v1",
  termsAccepted: true,
  engine: pack({ type: "ready" }),
  voices: [
    {
      id: "0",
      pack: pack({ type: "ready" }),
      characters: [
        { name: "四国めたん", credit: "VOICEVOX:四国めたん", styles: [{ id: "2", name: "ノーマル" }, { id: "0", name: "あまあま" }] },
        { name: "ずんだもん", credit: "VOICEVOX:ずんだもん", styles: [{ id: "3", name: "ノーマル" }] },
      ],
    },
    { id: "1", pack: pack(), characters: [{ name: "冥鳴ひまり", credit: "VOICEVOX:冥鳴ひまり", styles: [{ id: "14", name: "ノーマル" }] }] },
  ],
});

describe("speechErrorText", () => {
  it("内蔵エンジンの識別コードを、利用者向けの日本語にする", () => {
    expect(speechErrorText("SPEECH_CORE_NOT_READY")).toContain("未取得");
    expect(speechErrorText("invalid: SPEECH_CORE_LOAD_FAILED")).toContain("起動できません");
    expect(speechErrorText("SPEECH_CORE_UNSUPPORTED")).toContain("この環境");
    expect(speechErrorText("SPEECH_NO_VOICE")).toContain("声");
  });
  it("知らない文面は、そのまま返す", () => {
    expect(speechErrorText("SPEECH_ENGINE_UNREACHABLE: connect")).toBe("SPEECH_ENGINE_UNREACHABLE: connect");
    expect(speechErrorText("")).toBe("");
  });
});

describe("creditFor", () => {
  it("声の ID(スタイル ID)から、そのキャラのクレジットを引く", () => {
    expect(creditFor(info(), "3")).toBe("VOICEVOX:ずんだもん");
    expect(creditFor(info(), "0")).toBe("VOICEVOX:四国めたん");
  });
  it("未取得の声でも引ける。見つからない・空・情報なしは null", () => {
    expect(creditFor(info(), "14")).toBe("VOICEVOX:冥鳴ひまり");
    expect(creditFor(info(), "999")).toBeNull();
    expect(creditFor(info(), "")).toBeNull();
    expect(creditFor(null, "3")).toBeNull();
    expect(creditFor(undefined, "3")).toBeNull();
  });
});

describe("installedVoices", () => {
  it("取得済みの VVM の声だけを、Rust と同じ並び・同じ表示名で返す", () => {
    expect(installedVoices(info())).toEqual([
      { id: "2", label: "四国めたん（ノーマル）" },
      { id: "0", label: "四国めたん（あまあま）" },
      { id: "3", label: "ずんだもん（ノーマル）" },
    ]);
  });
  it("情報なし・取得中・失敗の VVM は含めない", () => {
    expect(installedVoices(null)).toEqual([]);
    const i = info();
    i.voices[0].pack = pack({ type: "downloading", downloaded: 1, total: 2 });
    expect(installedVoices(i)).toEqual([]);
    i.voices[0].pack = pack({ type: "failed", reason: "x" });
    expect(installedVoices(i)).toEqual([]);
  });
});

describe("voiceCharactersLabel", () => {
  it("キャラ名を、重複なしで「、」でつなぐ", () => {
    expect(voiceCharactersLabel(info().voices[0])).toBe("四国めたん、ずんだもん");
    const v = info().voices[0];
    v.characters.push({ ...v.characters[0] });
    expect(voiceCharactersLabel(v)).toBe("四国めたん、ずんだもん");
  });
});
```

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cd frontend && npx vitest run src/lib/speechCore.test.ts 2>&1 | tail -8`
Expected: `Failed to resolve import "./speechCore"`。

- [ ] **Step 3: 実装する**

`frontend/src/lib/speechCore.ts`:

```ts
// 内蔵 VOICEVOX CORE(Issue #625 段階2)の、表示用の純粋な関数。状態は Rust が持ち、ここは文面とクレジットの引き当てだけ。
import type { SpeechCoreInfo, Voice, VoicePackInfo } from "../bindings/tauri.gen";

/// 読み上げの状態のエラー文(識別コードを含む)を、利用者向けの日本語にする。知らない文面はそのまま返す。
const ERROR_TEXT: [string, string][] = [
  ["SPEECH_CORE_NOT_READY", "内蔵エンジンまたは選んだ声が未取得です。設定 → 読み上げ で取得してください"],
  ["SPEECH_CORE_LOAD_FAILED", "内蔵エンジンを起動できません。エンジン一式を取得し直してください"],
  ["SPEECH_CORE_UNSUPPORTED", "この環境では内蔵エンジンを使えません"],
  ["SPEECH_NO_VOICE", "声が選ばれていません"],
];

export function speechErrorText(message: string): string {
  for (const [code, text] of ERROR_TEXT) if (message.includes(code)) return text;
  return message;
}

/// 声の ID(スタイル ID)から、そのキャラのクレジット表記を引く。未取得の声でも引ける。見つからなければ null。
export function creditFor(info: SpeechCoreInfo | null | undefined, voiceId: string): string | null {
  if (!info || !voiceId) return null;
  for (const v of info.voices) for (const c of v.characters) if (c.styles.some((s) => s.id === voiceId)) return c.credit;
  return null;
}

/// 取得済みの VVM の声(設定画面の選択肢)。Rust の `CoreEngine::voices()` と同じ並び・同じ表示名。
export function installedVoices(info: SpeechCoreInfo | null | undefined): Voice[] {
  if (!info) return [];
  const out: Voice[] = [];
  for (const v of info.voices) {
    if (v.pack.status.type !== "ready") continue;
    for (const c of v.characters) for (const s of c.styles) out.push({ id: s.id, label: `${c.name}（${s.name}）` });
  }
  return out;
}

/// 1 つの VVM に入っているキャラ名(重複なし)。
export function voiceCharactersLabel(v: VoicePackInfo): string {
  return [...new Set(v.characters.map((c) => c.name))].join("、");
}
```

- [ ] **Step 4: テストが通ることを確かめる**

Run: `cd frontend && npx vitest run src/lib/speechCore.test.ts 2>&1 | tail -6`
Expected: 全部通る。

- [ ] **Step 5: 規約の確認ダイアログのテストを書く**

`frontend/src/ui/settings/SpeechCoreTermsDialog.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

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

const { default: SpeechCoreTermsDialog } = await import("./SpeechCoreTermsDialog.svelte");

const terms = { version: "v1", voiceModels: "# VOICEVOX 音声モデル 利用規約\n\nクレジット表記が必要です", onnxruntime: "# VOICEVOX ONNX Runtime 利用規約\n\n再配布できます" };

afterEach(cleanup);

describe("SpeechCoreTermsDialog", () => {
  it("2 つの規約の全文・クレジットの注意・取得するサイズを出す", () => {
    const { getByTestId } = render(SpeechCoreTermsDialog, { terms, sizeBytes: 59_308_488, onAccept: vi.fn(), onCancel: vi.fn() });
    const text = getByTestId("speech-terms-dialog").textContent ?? "";
    expect(text).toContain("VOICEVOX 音声モデル 利用規約");
    expect(text).toContain("VOICEVOX ONNX Runtime 利用規約");
    expect(text).toContain("クレジット");
    expect(text).toContain("59.3 MB");
  });

  it("同意と取りやめは、それぞれのコールバックだけを呼ぶ。初期フォーカスは取りやめ側(Enter 一発で同意にならない)", async () => {
    const onAccept = vi.fn();
    const onCancel = vi.fn();
    const { getByTestId } = render(SpeechCoreTermsDialog, { terms, sizeBytes: 1_000_000, onAccept, onCancel });
    expect(document.activeElement).toBe(getByTestId("speech-terms-cancel"));
    await fireEvent.click(getByTestId("speech-terms-cancel"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onAccept).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("speech-terms-accept"));
    expect(onAccept).toHaveBeenCalledTimes(1);
  });

  it("取得を始めている間は、同意を押せない", () => {
    const { getByTestId } = render(SpeechCoreTermsDialog, { terms, sizeBytes: 1, busy: true, onAccept: vi.fn(), onCancel: vi.fn() });
    expect((getByTestId("speech-terms-accept") as HTMLButtonElement).disabled).toBe(true);
  });
});
```

- [ ] **Step 6: ダイアログを実装する**

`frontend/src/ui/settings/SpeechCoreTermsDialog.svelte`(`LicenseConsentDialog.svelte` と同じ作り: portal、取りやめに初期フォーカス、モバイルでは戻るボタンで閉じる):

```svelte
<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { app } from "../../lib/store.svelte";
  import { pushModalHistory } from "../../lib/modalHistory";
  import { formatBytes } from "../../lib/localModel";
  import type { SpeechCoreTerms } from "../../lib/ipc";

  // VOICEVOX の利用規約(音声モデル・ONNX Runtime)への同意ダイアログ(Issue #625 段階2)。強制は Rust 側
  // (start_speech_core_download が版を検証する)で、ここは同意を取る画面。
  let {
    terms,
    sizeBytes,
    busy = false,
    onAccept,
    onCancel,
  }: {
    terms: SpeechCoreTerms;
    sizeBytes: number;
    busy?: boolean;
    onAccept: () => void;
    onCancel: () => void;
  } = $props();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  // 開いた時に取りやめへフォーカスする(Enter 一発で同意にならない)。
  let cancelButton = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    cancelButton?.focus();
  });

  // モバイルUIでは戻るボタンで、このダイアログだけを閉じる(LicenseConsentDialog と同じ)。
  $effect(() => {
    if (!app.useMobileUi()) return;
    return pushModalHistory(() => onCancel());
  });
</script>

<div
  class="fixed inset-0 grid items-start justify-items-center bg-black/45 pt-[max(8vh,var(--safe-top))]"
  style="z-index:1000"
  use:portal
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(560px,92vw)] rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-labelledby="speech-terms-title"
    tabindex="-1"
    data-testid="speech-terms-dialog"
  >
    <header id="speech-terms-title" class="mb-2.5 font-semibold">VOICEVOX の利用規約への同意</header>
    <p class="mb-2 mt-0 text-sm text-foreground">
      内蔵の読み上げエンジンは、VOICEVOX の音声モデルと ONNX Runtime を使います。取得するには、次の規約への同意が必要です。
      声を使った音声を公開する場合は、「VOICEVOX:キャラクター名」のクレジット表記と、各キャラクターの利用規約の遵守が必要です。
    </p>
    <p class="mb-2 mt-0 text-sm text-foreground">取得するファイルは約 {formatBytes(sizeBytes)} です。規約が更新されたときは、次の取得の前に、再度同意を求めます。</p>
    <div class="mb-3 max-h-[40vh] overflow-auto rounded-lg border border-border bg-muted p-2.5">
      <pre class="m-0 whitespace-pre-wrap break-words font-[inherit] text-xs text-foreground" data-testid="speech-terms-text">{terms.voiceModels}

{terms.onnxruntime}</pre>
    </div>
    <div class="flex justify-end gap-2">
      <Button bind:ref={cancelButton} variant="secondary" size="sm" onclick={onCancel} data-testid="speech-terms-cancel">キャンセル</Button>
      <Button size="sm" onclick={onAccept} disabled={busy} data-testid="speech-terms-accept">同意して取得</Button>
    </div>
  </div>
</div>
```

- [ ] **Step 7: エンジン一式と声の管理のテストを書く**

`frontend/src/ui/settings/SpeechCoreSection.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { commands, events } from "../../lib/ipc";
import type { PackInfo, SpeechCoreInfo } from "../../lib/ipc";

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

const { default: SpeechCoreSection } = await import("./SpeechCoreSection.svelte");

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const err = (message: string) => ({ status: "error" as const, error: { kind: "invalid" as const, message } });
const pack = (status: PackInfo["status"] = { type: "notInstalled" }, sizeBytes = 1_000_000): PackInfo => ({ status, sizeBytes, partialBytes: 0 });
const TERMS = { version: "v1", voiceModels: "音声モデルの規約", onnxruntime: "ONNX Runtime の規約" };

const info = (over: Partial<SpeechCoreInfo> = {}): SpeechCoreInfo => ({
  supported: true,
  termsVersion: "v1",
  termsAccepted: false,
  engine: pack({ type: "notInstalled" }, 31_870_280),
  voices: [
    {
      id: "0",
      pack: pack({ type: "notInstalled" }, 59_308_488),
      characters: [
        { name: "四国めたん", credit: "VOICEVOX:四国めたん", styles: [{ id: "2", name: "ノーマル" }] },
        { name: "ずんだもん", credit: "VOICEVOX:ずんだもん", styles: [{ id: "3", name: "ノーマル" }] },
      ],
    },
    { id: "1", pack: pack({ type: "notInstalled" }, 60_571_189), characters: [{ name: "冥鳴ひまり", credit: "VOICEVOX:冥鳴ひまり", styles: [{ id: "14", name: "ノーマル" }] }] },
  ],
  ...over,
});

let emit: (i: SpeechCoreInfo) => void = () => {};
const unlisten = vi.fn();

beforeEach(() => {
  unlisten.mockReset();
  vi.spyOn(commands, "getSpeechCoreTerms").mockResolvedValue(ok(TERMS) as never);
  vi.spyOn(events.speechCoreStatusChanged, "listen").mockImplementation((async (cb: (e: { payload: { info: SpeechCoreInfo } }) => void) => {
    emit = (i) => cb({ payload: { info: i } });
    return unlisten;
  }) as never);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SpeechCoreSection", () => {
  it("対応しない環境では、理由だけを出して、取得の操作を出さない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ supported: false })));
    const { findByTestId, queryByTestId } = render(SpeechCoreSection);
    expect((await findByTestId("speech-core-unsupported")).textContent).toContain("この環境");
    expect(queryByTestId("speech-core-engine-get")).toBeNull();
    expect(queryByTestId("speech-core-voice-0-get")).toBeNull();
  });

  it("未取得: エンジン一式のサイズが出て、声の取得はエンジン一式の取得後まで押せない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
    const { findByTestId, getByTestId } = render(SpeechCoreSection);
    expect((await findByTestId("speech-core-engine-status")).textContent).toContain("未取得");
    expect(getByTestId("speech-core-section").textContent).toContain("31.9 MB");
    expect((getByTestId("speech-core-engine-get") as HTMLButtonElement).disabled).toBe(false);
    expect((getByTestId("speech-core-voice-0-get") as HTMLButtonElement).disabled).toBe(true);
    expect(getByTestId("speech-core-voice-0-label").textContent).toContain("四国めたん、ずんだもん");
    expect(getByTestId("speech-core-voice-0-label").textContent).toContain("59.3 MB");
  });

  it("未同意で取得を押すと規約が出て、同意するまで開始しない。取りやめても開始しない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
    const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(SpeechCoreSection);
    await fireEvent.click(await findByTestId("speech-core-engine-get"));
    expect((await findByTestId("speech-terms-dialog")).textContent).toContain("音声モデルの規約");
    expect(start).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("speech-terms-cancel"));
    expect(queryByTestId("speech-terms-dialog")).toBeNull();
    expect(start).not.toHaveBeenCalled();
  });

  it("同意すると、対象と現行の規約の版を渡して開始する", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
    const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(SpeechCoreSection);
    await fireEvent.click(await findByTestId("speech-core-engine-get"));
    await fireEvent.click(await findByTestId("speech-terms-accept"));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("engine", "v1"));
    await vi.waitFor(() => expect(queryByTestId("speech-terms-dialog")).toBeNull());
    expect(getByTestId("speech-core-section")).toBeTruthy();
  });

  it("同意済みの版なら、規約を出さずに開始する(声の取得は、その VVM の ID を渡す)", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ termsAccepted: true, engine: pack({ type: "ready" }) })));
    const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, queryByTestId } = render(SpeechCoreSection);
    const get = (await findByTestId("speech-core-voice-1-get")) as HTMLButtonElement;
    expect(get.disabled).toBe(false);
    await fireEvent.click(get);
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("1", "v1"));
    expect(queryByTestId("speech-terms-dialog")).toBeNull();
  });

  it("開始が拒否されたら、理由を表示する", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ termsAccepted: true })));
    vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(err("すでにダウンロード中です"));
    const { findByTestId } = render(SpeechCoreSection);
    await fireEvent.click(await findByTestId("speech-core-engine-get"));
    expect((await findByTestId("speech-core-error")).textContent).toContain("すでにダウンロード中です");
  });

  it("Event の状態で表示が変わる: 取得中は進捗とキャンセル、ほかの取得は押せない", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ termsAccepted: true, engine: pack({ type: "ready" }) })));
    const cancel = vi.spyOn(commands, "cancelSpeechCoreDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId } = render(SpeechCoreSection);
    await findByTestId("speech-core-engine-status");
    const next = info({ termsAccepted: true, engine: pack({ type: "ready" }) });
    next.voices[0].pack = pack({ type: "downloading", downloaded: 29_654_244, total: 59_308_488 });
    emit(next);
    await vi.waitFor(() => expect(getByTestId("speech-core-voice-0-status").textContent).toContain("50%"));
    expect((getByTestId("speech-core-voice-1-get") as HTMLButtonElement).disabled, "取得は同時に 1 つ").toBe(true);
    await fireEvent.click(getByTestId("speech-core-voice-0-cancel"));
    await vi.waitFor(() => expect(cancel).toHaveBeenCalled());
  });

  it("取得済みの声は削除でき、押すと対象の ID で削除を呼ぶ。失敗した声は取得し直せる", async () => {
    const i = info({ termsAccepted: true, engine: pack({ type: "ready" }) });
    i.voices[0].pack = pack({ type: "ready" });
    i.voices[1].pack = pack({ type: "failed", reason: "通信が途切れました" });
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(i));
    const del = vi.spyOn(commands, "deleteSpeechCorePack").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId } = render(SpeechCoreSection);
    expect((await findByTestId("speech-core-voice-1-status")).textContent).toContain("通信が途切れました");
    expect((getByTestId("speech-core-voice-1-get") as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(getByTestId("speech-core-voice-0-delete"));
    await vi.waitFor(() => expect(del).toHaveBeenCalledWith("0"));
  });

  it("購読を解除して閉じる。取得の応答より新しい Event を、古い応答で上書きしない", async () => {
    let resolveInfo: (v: unknown) => void = () => {};
    const getInfo = vi.spyOn(commands, "getSpeechCoreInfo").mockReturnValue(new Promise((r) => (resolveInfo = r)) as never);
    const { getByTestId, unmount } = render(SpeechCoreSection);
    // 購読と、状態の取得の要求の両方が済んでから、Event を流す(取得の要求より前に流すと、「古い応答」にならない)。
    await vi.waitFor(() => expect(events.speechCoreStatusChanged.listen).toHaveBeenCalled());
    await vi.waitFor(() => expect(getInfo).toHaveBeenCalled());
    const newer = info({ engine: pack({ type: "ready" }) });
    emit(newer);
    resolveInfo(ok(info())); // 応答は、Event より古いスナップショット。
    await vi.waitFor(() => expect(getByTestId("speech-core-engine-status").textContent).toContain("取得済み"));
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
```

- [ ] **Step 8: テストが失敗することを確かめる**

Run: `cd frontend && npx vitest run src/ui/settings/SpeechCoreTermsDialog.test.ts src/ui/settings/SpeechCoreSection.test.ts 2>&1 | tail -8`
Expected: `Failed to resolve import "./SpeechCoreSection.svelte"`。

- [ ] **Step 9: `SpeechCoreSection.svelte` を実装する**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { commands, events, unwrap } from "../../lib/ipc";
  import type { PackInfo, SpeechCoreInfo, SpeechCoreTerms } from "../../lib/ipc";
  import { formatBytes, statusText } from "../../lib/localModel";
  import { voiceCharactersLabel } from "../../lib/speechCore";
  import SpeechCoreTermsDialog from "./SpeechCoreTermsDialog.svelte";

  // 内蔵 VOICEVOX CORE のエンジン一式と、声(VVM)の取得・削除(Issue #625 段階2)。
  // 状態はマウント時に Rust から取り、変化は Event(スナップショット)で受ける。設定画面を閉じても取得は続く。
  // 親(SpeechSection)が、取得済みの声の選択肢とクレジットを作れるよう、`info` を公開する。
  let { info = $bindable<SpeechCoreInfo | null>(null) }: { info?: SpeechCoreInfo | null } = $props();

  let err = $state<string | null>(null);
  /// 規約の確認を出している間だけ非 null。
  let terms = $state<SpeechCoreTerms | null>(null);
  let pending = $state<{ pack: string; sizeBytes: number } | null>(null);
  let starting = $state(false);

  // Event を受けるたびに進める。refresh の応答が、それより新しい Event の状態を古いスナップショットで上書きしないために使う。
  let eventSeq = 0;

  async function refresh() {
    const seqAtRequest = eventSeq;
    const i = await unwrap(commands.getSpeechCoreInfo());
    if (i && eventSeq === seqAtRequest) info = i;
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    (async () => {
      // 状態の取得より先に購読する(取得の間に届いた終了イベントを取りこぼして、表示が固まるのを避ける)。
      try {
        const u = await events.speechCoreStatusChanged.listen((e) => {
          eventSeq++;
          info = e.payload.info;
        });
        if (disposed) u();
        else unlisten = u;
      } catch (e) {
        err = String(e);
      }
      if (disposed) return;
      try {
        await refresh();
      } catch (e) {
        err = String(e);
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  const busy = $derived(
    !!info &&
      [info.engine, ...info.voices.map((v) => v.pack)].some((p) => p.status.type === "downloading" || p.status.type === "verifying"),
  );
  const engineReady = $derived(info?.engine.status.type === "ready");

  async function start(pack: string) {
    if (starting || !info) return;
    err = null;
    starting = true;
    try {
      await unwrap(commands.startSpeechCoreDownload(pack, info.termsVersion));
      terms = null;
      pending = null;
      await refresh();
    } catch (e) {
      terms = null;
      pending = null;
      err = String(e);
    } finally {
      starting = false;
    }
  }

  /// 同意済みの版なら、そのまま開始する。未同意(規約が変わったときを含む)なら、規約を出して同意を取る。
  async function onGet(pack: string, sizeBytes: number) {
    if (!info || starting) return;
    err = null;
    if (info.termsAccepted) {
      await start(pack);
      return;
    }
    try {
      terms = await unwrap(commands.getSpeechCoreTerms());
      pending = { pack, sizeBytes };
    } catch (e) {
      err = String(e);
    }
  }

  async function cancel() {
    try {
      await unwrap(commands.cancelSpeechCoreDownload());
    } catch (e) {
      err = String(e);
    }
  }

  async function remove(pack: string) {
    err = null;
    try {
      await unwrap(commands.deleteSpeechCorePack(pack));
      await refresh();
    } catch (e) {
      err = String(e);
    }
  }

  const canGet = (p: PackInfo) => p.status.type === "notInstalled" || p.status.type === "failed";
</script>

<div class="mb-3 rounded-lg border border-border p-3" data-testid="speech-core-section">
  {#if info && !info.supported}
    <p class="m-0 text-sm text-muted-foreground" data-testid="speech-core-unsupported">
      この環境では、内蔵の読み上げエンジンを使えません(対応: Linux・macOS・Windows の x64 / Linux と macOS の ARM)。外部のエンジンを使ってください。
    </p>
  {:else if info}
    <div class="mb-2 flex flex-wrap items-center gap-2 text-sm">
      <span class="font-medium" data-testid="speech-core-engine-label">エンジン一式(約 {formatBytes(info.engine.sizeBytes)})</span>
      <span class="text-xs text-muted-foreground" data-testid="speech-core-engine-status">{statusText(info.engine.status)}</span>
      {#if canGet(info.engine)}
        <Button variant="outline" size="sm" disabled={busy || starting} onclick={() => onGet("engine", info!.engine.sizeBytes)} data-testid="speech-core-engine-get">取得</Button>
      {:else if info.engine.status.type === "downloading"}
        <Button variant="outline" size="sm" onclick={cancel} data-testid="speech-core-engine-cancel">キャンセル</Button>
      {:else if info.engine.status.type === "ready"}
        <Button variant="outline" size="sm" disabled={busy} onclick={() => remove("engine")} data-testid="speech-core-engine-delete">削除</Button>
      {/if}
    </div>
    <p class="mb-2 mt-0 text-xs text-muted-foreground">エンジン一式は、CORE・ONNX Runtime・辞書です。声(VVM)は、使うものだけを取得します。</p>
    {#if !engineReady}
      <p class="mb-2 mt-0 text-xs text-muted-foreground" data-testid="speech-core-voices-hint">声の取得は、エンジン一式を取得してからできます。</p>
    {/if}
    <ul class="m-0 list-none p-0">
      {#each info.voices as v (v.id)}
        <li class="flex flex-wrap items-center gap-2 border-t border-border py-1.5 text-sm">
          <span class="min-w-0 flex-1 break-words" data-testid="speech-core-voice-{v.id}-label">
            {voiceCharactersLabel(v)}
            <span class="text-xs text-muted-foreground">(約 {formatBytes(v.pack.sizeBytes)})</span>
          </span>
          <span class="text-xs text-muted-foreground" data-testid="speech-core-voice-{v.id}-status">{statusText(v.pack.status)}</span>
          {#if canGet(v.pack)}
            <Button variant="outline" size="sm" disabled={!engineReady || busy || starting} onclick={() => onGet(v.id, v.pack.sizeBytes)} data-testid="speech-core-voice-{v.id}-get">取得</Button>
          {:else if v.pack.status.type === "downloading"}
            <Button variant="outline" size="sm" onclick={cancel} data-testid="speech-core-voice-{v.id}-cancel">キャンセル</Button>
          {:else if v.pack.status.type === "ready"}
            <Button variant="outline" size="sm" disabled={busy} onclick={() => remove(v.id)} data-testid="speech-core-voice-{v.id}-delete">削除</Button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="m-0 text-sm text-muted-foreground">読み込み中…</p>
  {/if}
  {#if err}<p class="my-2 break-words text-sm text-destructive" data-testid="speech-core-error">{err}</p>{/if}
</div>

{#if terms && pending}
  <SpeechCoreTermsDialog
    {terms}
    sizeBytes={pending.sizeBytes}
    busy={starting}
    onAccept={() => start(pending!.pack)}
    onCancel={() => {
      terms = null;
      pending = null;
    }}
  />
{/if}
```

- [ ] **Step 10: テストが通ることを確かめる**

Run: `cd frontend && npx vitest run src/lib/speechCore.test.ts src/ui/settings/SpeechCoreTermsDialog.test.ts src/ui/settings/SpeechCoreSection.test.ts 2>&1 | tail -12 && pnpm check 2>&1 | tail -4`
Expected: 全部通る。`pnpm check` はエラー 0。

調整が要りそうな箇所: `statusText` の「ダウンロード中 … (50%)」の文面に `50%` が含まれること(`lib/localModel.ts`)。`$bindable<…>(null)` の型引数の書き方が `svelte-check` に通らなければ、`let { info = $bindable(null) }: { info?: SpeechCoreInfo | null } = $props();` にする。

- [ ] **Step 11: コミットする**

```sh
git add frontend/src
git commit -m "feat(tts): 内蔵エンジンの取得・削除と規約確認のUIを追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: 設定画面のエンジン切替と、カラムの声の選択肢

**Files:**
- Modify: `frontend/src/ui/settings/SpeechSection.svelte`、`frontend/src/ui/settings/SpeechSection.test.ts`
- Modify: `frontend/src/ui/AddColumnModal.svelte`、`frontend/src/ui/AddColumnModal.test.ts`

**Interfaces:**
- Consumes: Task 10 の `SpeechCoreSection`(`bind:info`)、`installedVoices`・`creditFor`・`speechErrorText`、`commands.listSpeechCoreVoices()`、`app.speech.engine`

- [ ] **Step 1: 失敗するテストを書く(設定画面)**

`SpeechSection.test.ts` の `describe("SpeechSection", …)` の末尾(閉じる `});` の前)に足す。ファイルの先頭の import に `events` を足す(`import { commands, events } from "../../lib/ipc";`)。

```ts
  describe("内蔵エンジン(Issue #625 段階2)", () => {
    const pack = (type: "ready" | "notInstalled") => ({ status: { type } as never, sizeBytes: 1000, partialBytes: 0 });
    const coreInfo = (ready = true) => ({
      supported: true,
      termsVersion: "v1",
      termsAccepted: true,
      engine: pack("ready"),
      voices: [
        {
          id: "0",
          pack: pack(ready ? "ready" : "notInstalled"),
          characters: [{ name: "ずんだもん", credit: "VOICEVOX:ずんだもん", styles: [{ id: "3", name: "ノーマル" }] }],
        },
      ],
    });

    beforeEach(() => {
      vi.spyOn(events.speechCoreStatusChanged, "listen").mockResolvedValue(() => {} as never);
    });

    it("既定は外部エンジン。内蔵に切り替えると、URL の入力が消えて、取得の部品が出る", async () => {
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { getByTestId, queryByTestId, findByTestId } = render(SpeechSection);
      expect((getByTestId("speech-engine-external") as HTMLInputElement).checked).toBe(true);
      expect(queryByTestId("speech-core-section")).toBeNull();
      await fireEvent.click(getByTestId("speech-engine-core"));
      expect(await findByTestId("speech-core-section")).toBeTruthy();
      expect(queryByTestId("speech-url")).toBeNull();
    });

    it("内蔵で保存すると、engine: voicevoxCore を渡す(URL は空でもよい)", async () => {
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const spy = vi.spyOn(app, "setSpeechSettings").mockResolvedValue(undefined);
      const { getByTestId, getByText } = render(SpeechSection);
      await fireEvent.click(getByTestId("speech-engine-core"));
      await fireEvent.click(getByText("保存"));
      await vi.waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ engine: "voicevoxCore", externalUrl: "" })));
    });

    it("声の選択肢は取得済みの VVM の声だけで、選んだ声のクレジットが出る", async () => {
      app.speech = { ...app.speech, engine: "voicevoxCore", defaultVoiceId: "3" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { findByTestId, getByTestId } = render(SpeechSection);
      await vi.waitFor(() => {
        const options = Array.from((getByTestId("speech-voice") as HTMLSelectElement).options).map((o) => o.textContent?.trim());
        expect(options).toContain("ずんだもん（ノーマル）");
      });
      expect((await findByTestId("speech-core-credit")).textContent).toContain("VOICEVOX:ずんだもん");
    });

    it("取得済みの声がなければ、取得を促す(接続テストは出さない)", async () => {
      app.speech = { ...app.speech, engine: "voicevoxCore" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo(false)) as never);
      const { findByText, queryByTestId } = render(SpeechSection);
      expect(await findByText(/声を取得してください/)).toBeTruthy();
      expect(queryByTestId("speech-connect-test")).toBeNull();
    });

    it("内蔵のエラー状態は、識別コードではなく日本語で出す", () => {
      app.speech = { ...app.speech, engine: "voicevoxCore" };
      app.speechStatus = { type: "error", message: "invalid: SPEECH_CORE_NOT_READY" };
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(coreInfo()) as never);
      const { getByTestId } = render(SpeechSection);
      expect(getByTestId("speech-status").textContent).toContain("未取得");
      expect(getByTestId("speech-status").textContent).not.toContain("SPEECH_CORE_NOT_READY");
    });
  });
```

また、`beforeEach` の `app.speech = {…}` の初期化で、他のテストへの漏れを防ぐため、`engine: "external"` に戻していることを確かめる(既存のとおり)。

- [ ] **Step 2: テストが失敗することを確かめる**

Run: `cd frontend && npx vitest run src/ui/settings/SpeechSection.test.ts 2>&1 | tail -10`
Expected: `speech-engine-external` が見つからない、など。

- [ ] **Step 3: `SpeechSection.svelte` を実装する**

`<script>` の変更:
  - import に足す: `import SpeechCoreSection from "./SpeechCoreSection.svelte";`、`import { creditFor, installedVoices, speechErrorText } from "../../lib/speechCore";`、`import type { SpeechCoreInfo, SpeechEngineKind } from "../../lib/ipc";`
  - 状態に足す(`let enabled = …` の並び):
    ```ts
      let engine = $state<SpeechEngineKind>(app.speech.engine);
      /// 内蔵エンジンの状態(`SpeechCoreSection` が埋める)。取得済みの声の選択肢とクレジットに使う。
      let coreInfo = $state<SpeechCoreInfo | null>(null);
    ```
  - `statusText` の `$derived` を、エンジン別にする:
    ```ts
      const statusText = $derived.by(() => {
        const s = app.speechStatus;
        if (engine === "voicevoxCore") {
          if (s.type === "ok") return "内蔵エンジンで読み上げられています";
          if (s.type === "error") return `読み上げに失敗しています(${speechErrorText(s.message)})`;
          return "";
        }
        if (s.type === "ok") return "エンジンに接続できています";
        if (s.type === "error") return `エンジンに接続できません(${s.message})`;
        return "";
      });
      /// 声の選択肢。外部は接続テストで取得したもの、内蔵は取得済みの VVM の声。
      const voiceOptions = $derived(engine === "voicevoxCore" ? installedVoices(coreInfo) : voices);
      const credit = $derived(engine === "voicevoxCore" ? creditFor(coreInfo, defaultVoiceId) : null);
    ```
  - `save()` の `engine: "external",` を `engine,` にする。

テンプレートの変更:
  - 「全体スイッチ」の説明の次(URL 入力の前)に、エンジンの選択を足す:
    ```svelte
    <fieldset class="mb-3 mt-0 flex flex-wrap gap-x-4 gap-y-1 border-0 p-0 text-sm">
      <legend class="mb-1 text-muted-foreground">音声合成エンジン</legend>
      <label class="flex items-center gap-1.5">
        <input type="radio" name="speech-engine" value="external" data-testid="speech-engine-external" bind:group={engine} /> 外部のエンジン(VOICEVOX ENGINE / AivisSpeech Engine)
      </label>
      <label class="flex items-center gap-1.5">
        <input type="radio" name="speech-engine" value="voicevoxCore" data-testid="speech-engine-core" bind:group={engine} /> 内蔵の VOICEVOX CORE
      </label>
    </fieldset>
    ```
  - 既存の「URL の入力〜接続テスト〜注意書き」のブロック(`<label …>音声合成エンジンの URL…` から、Android の注意書きの `</p>` まで)を `{#if engine === "external"} … {:else} <SpeechCoreSection bind:info={coreInfo} /> {/if}` で囲む。状態表示(`{#if statusText}…{/if}`)は、`{#if}` の外に置く(両方で使う)。
  - 既定の声の `<select>` の中を、`voiceOptions` で描く:
    ```svelte
        {#if voiceOptions.length === 0}
          <option value={defaultVoiceId}>{defaultVoiceId ? `現在の声: ${defaultVoiceId}` : engine === "voicevoxCore" ? "声を取得してください" : "接続テストで声を取得してください"}</option>
        {:else}
          {#each voiceOptions as v (v.id)}<option value={v.id}>{v.label}</option>{/each}
        {/if}
    ```
    (選択中の声が `voiceOptions` に無い場合に `現在の声: …` を足す分岐があるなら、そのまま残す。)
  - 声の選択の次に足す:
    ```svelte
    {#if credit}
      <p class="my-1 mb-3 text-xs text-muted-foreground" data-testid="speech-core-credit">クレジット表記: {credit}</p>
    {/if}
    ```
  - 末尾のクレジットの注意書き(`data-testid="speech-credit"`)はそのまま残す。

- [ ] **Step 4: 設定画面のテストが通ることを確かめる**

Run: `cd frontend && npx vitest run src/ui/settings/SpeechSection.test.ts 2>&1 | tail -10`
Expected: 既存の 9 件と新規 5 件が通る。

- [ ] **Step 5: 失敗するテストを書く(カラムの声の選択肢)**

`AddColumnModal.test.ts` の `describe("AddColumnModal の読み上げ(Issue #625)", …)` の末尾に足す(`invokeMock` は、`invoke` の生の戻り値を返す):

```ts
  it("内蔵エンジンのときは、取得済みの声を list_speech_core_voices から取得して、選択肢に出す", async () => {
    app.speech = { ...app.speech, enabled: true, engine: "voicevoxCore", externalUrl: "" };
    invokeMock.mockImplementation(async (cmd: string) => (cmd === "list_speech_core_voices" ? [{ id: "3", label: "ずんだもん（ノーマル）" }] : null));
    const tab = setupEdit({ enabled: true, speakAuthor: true, queueLimit: 5, voiceId: "" });
    const { getByTestId } = render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    await waitFor(() => {
      const options = Array.from((getByTestId("speech-column-voice") as HTMLSelectElement).options).map((o) => o.textContent?.trim());
      expect(options).toContain("ずんだもん（ノーマル）");
    });
    expect(invokeMock.mock.calls.some((c) => c[0] === "list_speech_voices")).toBe(false);
  });

  it("外部エンジンで URL が空のときは、声の一覧を取得しない", async () => {
    app.speech = { ...app.speech, enabled: true, engine: "external", externalUrl: "" };
    invokeMock.mockResolvedValue(null);
    const tab = setupEdit({ enabled: true, speakAuthor: true, queueLimit: 5, voiceId: "" });
    render(AddColumnModal, { onclose: vi.fn(), groupId: null, editTab: tab });
    await new Promise((r) => setTimeout(r, 20));
    expect(invokeMock.mock.calls.some((c) => c[0] === "list_speech_voices" || c[0] === "list_speech_core_voices")).toBe(false);
  });
```
(`beforeEach` で `invokeMock.mockReset()` されていなければ、各テストの先頭で `invokeMock.mockReset();` を足す。)

- [ ] **Step 6: テストが失敗することを確かめ、実装する**

Run: `cd frontend && npx vitest run src/ui/AddColumnModal.test.ts 2>&1 | tail -10`
Expected: 新規の 1 件目が失敗する(`list_speech_core_voices` が呼ばれない)。

`AddColumnModal.svelte` の、声の選択肢を取得する `$effect` を次のとおりにする:

```ts
  // 声の選択肢は、エンジンが使えるときだけ取得する(失敗しても「既定の声を使う」だけで使える)。
  // 内蔵は取得済みの声、外部は設定済みの URL のエンジンから。
  $effect(() => {
    if (!speechEnabled) return;
    const core = app.speech.engine === "voicevoxCore";
    if (!core && !app.speech.externalUrl) return;
    let cancelled = false;
    (core ? commands.listSpeechCoreVoices() : commands.listSpeechVoices(app.speech.externalUrl))
      .then((r) => {
        if (!cancelled && r.status === "ok") speechVoices = r.data;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });
```

- [ ] **Step 7: すべてのフロントのテストと型を確かめる**

Run: `cd frontend && npx vitest run 2>&1 | tail -6 && pnpm check 2>&1 | tail -4`
Expected: 全部通る。`pnpm check` はエラー 0。

- [ ] **Step 8: コミットする**

```sh
git add frontend/src
git commit -m "feat(tts): 設定画面で外部と内蔵のエンジンを切り替え、声の選択肢を内蔵に対応させる(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 12: ドキュメント・実環境の確認・全体の検証

**Files:**
- Modify: `docs/guide/user-guide.md`、`CLAUDE.md`
- Modify: `docs/superpowers/plans/2026-11-20-issue-625-stage2-spike-notes.md`(確認結果の欄を埋める)

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md` の「### 読み上げ」の節の末尾に、次の内容の小節を足す(既存の文体・見出しの深さに合わせる。`grep -n "### 読み上げ" docs/guide/user-guide.md` で位置を見つける):
  - 「内蔵の VOICEVOX CORE」: 外部のエンジンを立てなくても読み上げられる(Linux・macOS・Windows の x64、Linux・macOS の ARM。Android は未対応)。設定 → 読み上げ で「内蔵の VOICEVOX CORE」を選び、「エンジン一式」(約 32 MB)を取得し、使う声(約 55〜75 MB ずつ。全部で約 1.65 GB)を取得する。取得の前に、VOICEVOX 音声モデルと ONNX Runtime の利用規約への同意が要る。規約が更新されたら、次の取得の前に再同意を求める。
  - クレジット: 声を使った音声を公開する場合は「VOICEVOX:キャラクター名」(キャラクターごとの表記は、声の選択の横に出る)と、各キャラクターの利用規約の遵守が必要。
  - 外部エンジンとの切り替え: 切り替えても、カラムの声の設定はそのまま使える(声の ID が共通)。取得していない声を選んでいるカラムは、読み上げられず、設定画面に「内蔵エンジンまたは選んだ声が未取得です」と出る。
  - 容量と削除: 声ごとに削除できる。エンジン一式を削除するときは、読み上げを使っていないこと(Windows では、一度使うと、アプリを再起動するまで削除できないことがある)。
  - 設定のエクスポートに、エンジンの選択と規約の同意は含まれない(インポートで同意済みにならない)。

- [ ] **Step 2: `CLAUDE.md` の `tts/` の記述を更新する**

`CLAUDE.md` の `- `tts/` — カラム単位のノート読み上げ(Issue #625、…` の段落の、「段階2で内蔵 VOICEVOX CORE を同じ trait で足す予定(別 spec)。」を、次の内容に置き換える(同じ段落の文体に合わせ、1 つの長い段落のまま足す):

「**内蔵 VOICEVOX CORE**(段階2、設計は `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`)は `tts/voicevox/` にある。公式の C API を実行時に取得して `libloading` で動的ロードする(Rust クレート `voicevox_core` の git 依存は採らない)。`catalog.rs`(固定値: CORE 0.17.1・ONNX Runtime 1.17.3・辞書・VVM 27 個。VVM の中身は `catalog.json`、`scripts/gen-voicevox-catalog.py` で再生成する。歌声だけの `s0` は含めない。規約の写しの SHA-256 が同意版)、`store.rs`(`app_data_dir/voicevox/` の配置。`READY` を最後に置いてからディレクトリをリネームする)、`archive.rs`(zip・tgz の展開。`..`・絶対パス・シンボリックリンク・巨大なファイルを拒否する)、`ffi.rs`(`CoreBackend` trait と、`cfg(desktop)` の `NativeCore`。**`unsafe` はここだけ**。ライブラリは `Box::leak` でプロセスの終わりまで外さない)、`engine.rs`(`CoreEngine: SpeechEngine`。声の ID はスタイル ID の数値で、外部エンジンと共通。10 分のアイドルで解放する)、`packs.rs`(取得・削除・進捗。`translate/download.rs` の `download()` を再利用。同時に走る取得は 1 つ)。**規約への同意は Rust 側で強制する**(`start_speech_core_download` が同意版を検証し、`SettingsData.speech_core_terms_version` は、エクスポートに含めず、インポートで現在の値を維持する)。ONNX Runtime は、CORE の推奨(1.23.2、prerelease)ではなく 1.17.3 を、パスを明示してロードする。`AppContext::engine()` が `settings.engine` で `ExternalEngine` と `CoreEngine` を切り替える。実物のテストは `#[ignore]`(`tts::voicevox::ffi::real_tests`、`tts::voicevox::tests::real_end_to_end`)。」

- [ ] **Step 3: 実物のエンドツーエンドのテストを書く**

`tts/voicevox/mod.rs` の末尾に足す(実際の GitHub から、エンジン一式と声 `0` を取得し、`CoreEngine` で合成する。ネットワークと約 100 MB のディスクが要る):

```rust
#[cfg(all(test, desktop))]
mod real_tests {
    use super::*;
    use crate::tts::engine::{SpeechEngine, SynthParams};
    use crate::translate::model_store::LocalModelStatus;
    use packs::PackKey;
    use std::time::Duration;

    /// 実物の取得 → 展開 → ロード → 合成を、通しで行う。
    /// `TSUMUGI_TEST_CORE_ROOT` に、ディスク上の空のディレクトリ(`/tmp` ではない)を渡す。
    /// `TSUMUGI_TEST_CORE_ROOT=$HOME/.cache/tsumugi-voicevox-e2e cargo test --lib tts::voicevox::real_tests -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn real_end_to_end() {
        let root = PathBuf::from(std::env::var("TSUMUGI_TEST_CORE_ROOT").expect("TSUMUGI_TEST_CORE_ROOT を設定してください"));
        let core = SpeechCore::new(root, reqwest::Client::new());
        let notify: packs::Notify = Arc::new(|| {});
        for key in [PackKey::Engine, PackKey::Voice("0".into())] {
            if let Err(e) = core.packs.start(key, notify.clone()) {
                // 取得済み(前回の実行の残り)なら、そのまま進む。
                eprintln!("start skipped: {e:?}");
            }
            tokio::time::timeout(Duration::from_secs(600), async {
                while core.packs.is_running() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .expect("取得が終わらない");
        }
        let snap = core.packs.snapshot();
        assert_eq!(snap.engine.status, LocalModelStatus::Ready, "{:?}", snap.engine.status);
        assert_eq!(snap.voices[0].pack.status, LocalModelStatus::Ready, "{:?}", snap.voices[0].pack.status);

        let wav = core.engine.synthesize("この音声は、ボイスボックスを使用して、出力されています。", "3", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert!(wav.len() > 100_000, "{}", wav.len());
        let fast = core.engine.synthesize("この音声は、ボイスボックスを使用して、出力されています。", "3", &SynthParams { speed_scale: 2.0, volume: 1.0 }).await.unwrap();
        assert!(fast.len() < wav.len() * 3 / 4, "話速 2.0 は、1.0 より十分短い: {} vs {}", fast.len(), wav.len());
        let quiet = core.engine.synthesize("あ", "3", &SynthParams { speed_scale: 1.0, volume: 0.1 }).await.unwrap();
        let loud = core.engine.synthesize("あ", "3", &SynthParams { speed_scale: 1.0, volume: 2.0 }).await.unwrap();
        let peak = |w: &[u8]| w[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap_or(0);
        assert!(peak(&quiet) < peak(&loud), "音量が効く: {} vs {}", peak(&quiet), peak(&loud));
        assert!(!core.engine.voices().await.unwrap().is_empty());

        // 取得していない声は、エンジンが動いていても、`ERR_NOT_READY`。
        let r = core.engine.synthesize("a", "14", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await;
        assert!(matches!(r, Err(crate::error::Error::Invalid(ref m)) if m == crate::tts::ERR_NOT_READY), "{r:?}");
    }
}
```

- [ ] **Step 4: 自動テストをすべて通す**

Run:
```sh
cd src-tauri && cargo test --lib 2>&1 | tail -6
cd ../frontend && npx vitest run 2>&1 | tail -5 && pnpm check 2>&1 | tail -3
```
Expected: cargo は全部通る(`ignored` は実物・実機のもの)。vitest・`pnpm check` も通る。件数をスパイクノートに残す。

- [ ] **Step 5: 実物のテストを走らせる(ネットワーク・約 100 MB)**

Run: `cd src-tauri && TSUMUGI_TEST_CORE_ROOT=$HOME/.cache/tsumugi-voicevox-e2e cargo test --lib tts::voicevox::real_tests -- --ignored --nocapture 2>&1 | tail -8`
Expected: `real_end_to_end` が通る(取得に数十秒〜数分)。**失敗したら先へ進まず**、原因を調べる(`systematic-debugging`)。結果(所要時間・話速と音量の効き)をスパイクノートに書く。

併せて、実物の照合のテストを走らせる: `cargo test --lib tts::voicevox::catalog -- --ignored 2>&1 | tail -5`(規約の写しが上流と一致すること)。

- [ ] **Step 6: 実アプリ(Xvfb)で、取得 → 合成 → 状態イベントを確かめる**

デバッグブリッジ(`CLAUDE.md` の `debug_bridge.rs` の節)で確かめる。**実画面に漏れないよう**、Xvfb + `dbus-run-session` で隔離し、`WAYLAND_DISPLAY` を unset し、起動したプロセスは、トークン方式の正確な PID で後始末する(メモリの `reference-isolated-instance-needs-short-xdg-path`・`feedback-dev-server-verification-must-use-virtual-display` に従う。`pkill` は使わない)。手順:
  1. 隔離したインスタンスを `cargo tauri dev` で起動する(リポジトリのルートから)。
  2. ブリッジで、`window.__TAURI_INTERNALS__.invoke("get_speech_core_info")` を呼び、`supported: true`、`termsAccepted: false` を確かめる。
  3. `start_speech_core_download` を、**誤った版**で呼び、拒否されること(`規約への同意が必要です`)と、`get_speech_core_info` が `termsAccepted: false` のままなことを確かめる。
  4. `get_speech_core_terms` の `version` で、`engine` を取得する。`SpeechCoreStatusChanged` の進捗を、ブリッジの kick-and-poll(`await` なし)で `window.__events` などに溜めて読み、`downloading` → `verifying` → `ready` の遷移を確かめる。続けて `"0"` を取得する。
  5. `set_speech_settings` で `{enabled: true, engine: "voicevoxCore", defaultVoiceId: "3", …}` を保存し、`test_speech` を呼ぶ。`get_speech_status` が `ok` になること(再生デバイスが無い Xvfb では、再生が失敗して `error` になりうる。その場合は、エラーの文面が、合成ではなく再生であること(`SPEECH_CORE_*` ではないこと)を確かめる)。
  6. `delete_speech_core_pack("0")` → 状態が `notInstalled` に戻ること。再度の `test_speech` が `SPEECH_CORE_NOT_READY` で失敗し、3 回連続で状態が `error` になり、メッセージを日本語に変換できること(設定画面の表示は、vitest で確認済み)。
  7. 終了時に、自分で起動したプロセスだけを、PID で止める。
結果を、スパイクノートの「実アプリ(Xvfb)での確認」に書く。**音が鳴るか(スピーカー)は確認できない**ので、「未確認」と明記する。

- [ ] **Step 7: スパイクノートを仕上げる**

`docs/superpowers/plans/2026-11-20-issue-625-stage2-spike-notes.md` の「実アプリ(Xvfb)での確認」と「`#[ignore]` の実物のテストの結果」の欄を埋める。未確認のもの(スピーカーの音、macOS・Windows、release ビルドでの実行、実機)を、箇条書きで残す。

- [ ] **Step 8: コミットする**

```sh
git add docs CLAUDE.md src-tauri/src/tts/voicevox/mod.rs
git commit -m "docs(tts): 内蔵エンジンのユーザーガイドとCLAUDE.mdを更新し、実物の通しテストを追加(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-Review(plan 作成時)

**1. Spec coverage**
- 範囲と取得物(§1): Task 1(カタログ・規約・実測)、Task 2(固定値・規約の版)。
- 規約と同意(§1): Task 2(版)・Task 9(Rust 側で強制、同意は開始後に記録)・Task 8(保存・エクスポートに含めない・インポートで維持)・Task 10(確認画面)。
- Rust の構成(§2): Task 3(store)・4(archive)・5(ffi)・6(engine)・7(packs)・9(接続)。`download()` の再利用は Task 7。
- データフロー・エラー・スレッド(§3): Task 6(声の ID → VVM → 合成、エラーの対応、`spawn_blocking`、スレッド数は Task 5 の `default_cpu_threads`)、Task 7(削除・更新・Windows の案内文)、Task 3(古い版の掃除)。ネイティブの panic は Task 6 のテスト。
- 設定と UI(§4): Task 8(型)・10(取得と管理)・11(エンジン切替・声の選択肢・クレジット)。`list_speech_core_voices` は Task 9(spec は Task 1 で訂正)。
- テストと検証(§5): 各タスクの TDD、実物のテストは Task 5・12、実アプリの確認は Task 12。確認できないものは Task 12 でスパイクノートに残す(PR にも書く)。
- ドキュメント: Task 12。
- 段階3への準備(環境の組を引く関数を 1 か所に): `engine_assets_for`(Task 2)。

**2. Placeholder scan**: 「TBD」「あとで」はない。外部の API の細部(`zip` 2・`tar` 0.4 のメソッド名、`lib.get` の型推論、`$bindable` の型引数)は、コンパイルエラーが出たときの手当てを、該当の Step に書いた。

**3. Type consistency**: `CoreBackend`(Task 5)のメソッド名は、`CoreEngine`(Task 6)の呼び出し・`Fake`(Task 6)・`NativeCore`(Task 5)で一致。`EngineAssets { core, ort, dict, core_lib, ort_lib }`(Task 2)は、`Store`(Task 3)・`Packs`(Task 7)・テスト(Task 6・7)で一致。`Asset.sha256` は `String`(Task 2)で、Task 7 の `ModelSpec` に `.clone()` で渡す。`PackKey::parse` の入力は `"engine"` か VVM の ID(Task 7)で、コマンドの引数(Task 9)・フロントの `onGet("engine" | v.id)`(Task 10)と一致。`SpeechCoreInfo` の項目(`supported`・`termsVersion`・`termsAccepted`・`engine`・`voices`)は Rust(Task 7)・TS の生成物・フロントの fixture(Task 10・11)で一致。`Store::write_ready` は Task 4 で足し、Task 4(`test_util`)・Task 7(`commit_engine`)が使う。

**4. Review Focus**: 5 件とも担当のタスクのテストに入れた(1 → Task 9 の 3 本、2 → Task 4・7、3 → Task 7、4 → Task 6、5 → Task 6・9)。
