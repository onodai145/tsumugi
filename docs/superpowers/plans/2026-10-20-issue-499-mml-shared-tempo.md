# MML のテンポ共有 `t=` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に `t=<n>`(全トラックの初期テンポを 1 回で指定する宣言)を足し、字句解析・「記法」ヘルプ・ガイドを追随させる。

**Architecture:** `src-tauri/src/mml.rs` だけを変える(`NoteEvent` / `MmlScore` / `MmlError` / レンダラーは変えない)。`parse_track` が `t=` を `Op::SharedTempo` として読み、`parse` は全トラックを先に構文解析して `t=` を 1 つに決め、`run_track` に初期テンポとして渡す。フロントは字句解析で `t=` を 1 つのコマンドにし、ヘルプとガイドに 1 行ずつ足す。

**Tech Stack:** Rust(`src-tauri`、`cargo test`)、Svelte 5 + TypeScript(`frontend`、Vitest + jsdom)。

**Spec:** `docs/superpowers/specs/2026-10-20-issue-499-mml-shared-tempo-design.md`(前提: `2026-10-15-issue-75-mml-ringtone-design.md` / `2026-10-20-issue-499-mml-syntax-extension-design.md`)

## Global Constraints

- 作業ブランチは `feat/issue-499-mml-shared-tempo`(作成済み)。`main` には触らない。リポジトリルートは `/home/onodai145/repos/github.com/onodai145/tsumugi`。コマンドは絶対パスで書く(作業ディレクトリが変わっても動くように)。
- v1 と PR #511 の記法・既定値・範囲は変えない。今まで通っていた MML は、同じ `MmlScore` になる(`t=` は今は構文エラーなので、保存済みの MML の意味は変わらない)。
- `NoteEvent` / `MmlScore` / `MmlError` / `Timbre` と `mmlRender.ts` は変えない。`frontend/src/bindings/tauri.gen.ts` は変わらない(変わったら何かがおかしい)。
- 上限は 5000 文字(コメントも含む)・3000 音・4 トラック・90 秒・展開後 `MAX_EXPANDED_OPS = 500_000` 命令(トラックごと)。値は変えない。
- `t=<n>` の仕様(spec が正):
  - `n` は 32〜255(`t` と同じ)。大文字・空白は今までどおり(`T = 60` も可)。書かなければ初期テンポは 120。
  - どのトラックに書いても全トラックの初期テンポになる。トラックの `t<n>` は、そのトラックだけを上書きする。
  - 置けるのは、そのトラックの最初の音符・休符・`^` より前で、`[ ]` と `{ }` の外。違えばエラー(位置は `t`)。1 回だけ(2 個目は、その `t` の位置でエラー)。
  - エラー位置は元のテキストの文字位置(Unicode スカラ値の添字、0 始まり)。UI は 1 始まりの「N 文字目」。`MmlError.position` は `u32`(テストで `usize` と比べるときは `as usize`)。
- コミットメッセージは件名のみ(本文・箇条書きなし)。ハーネスの指示どおり、末尾に `Co-Authored-By` トレーラーを付ける(下の例は `-m` を 2 回に分けている)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら(GPG の署名がタイムアウトすることがある)、止めてユーザーに報告する(再試行しない)。
- Rust のテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::<名前>`。単一のフロントテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run <パス>`(`pnpm test -- <パス>` はパスを無視して全件走る)。
- `rustfmt --check --edition 2021 src/mml.rs` には、`main` にも既にある 5 箇所の差分がある(サンプルと音色のテスト)。確認は「差分の数が増えていない(5 のまま)」で行い、既存の部分は整形しない(`cargo fmt` を全体に掛けない)。
- PR を作るときは、本文に `Fixes #499` / `Closes #499` を書かない(#499 には「中くらい」の候補が未着手で残る)。`Refs #499` と書く。Issue のチェックボックスを更新するのは、ユーザーの確認を取ってから。
- ユーザーに見せる文章(最終報告・質問)は日本語。

## Review Focus

仕様が黙っているが、使う人が踏みやすい入力と条件。それぞれ、持ち主のタスクにテストを足してある。

1. 4 トラックがそれぞれ展開上限(50 万命令)に近い入力。全トラックを先に構文解析するので、4 本分の命令が同時にメモリに載る(今までは 1 本ずつ)。短時間で終わること。メモリのピークは測らない(未確認として spec に書く)。(Task 1)
2. `t=` の位置と個数: 音符・休符の後、2 本目の音符の後、繰り返し・連符の中、同じトラックに 2 個、別のトラックに 2 個、数字なし、範囲外。位置が元のテキストのまま。(Task 1)
3. `t=` とトラックの `t<n>` の関係: 先頭の `t120`、途中の `t120`、`t=` を 2 本目に書いた場合。(Task 1)
4. エラー報告順の変更: 後ろのトラックの構文エラーが、前のトラックの実行時エラー(90 秒超過)より先に出る。エラーが 1 つの入力は今までどおり。(Task 1)
5. `t=` だけのトラック(`t=60 ; c ; d`)がトラックに数えられること(3 トラック、音は 2 本目から)。(Task 1)
6. v1 と PR #511 の MML が同じ結果になること(既存テストが全部そのまま通る)。(Task 1 の全件実行)
7. フロント: `T = 140` のように空白をまたぐ `t=` が 1 つのコマンドとして色分けされること、今までの `t120` は変わらないこと。(Task 2)

---

### Task 1: Rust — `t=` を読み、全トラックの初期テンポにする

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(`Op`、`parse_track` の `'t'`、`shared_tempo` の追加、`run_track` のシグネチャと初期化と `match`、`parse`、tests)

**Interfaces:**
- Consumes: `parse_track(syms, end) -> Result<Vec<Item>, MmlError>`、`Item { op, pos }`、`Cursor::peek` / `bump` / `number() -> Option<(u32, usize)>`、`DEFAULT_TEMPO: u32 = 120`、`MAX_TRACKS`、`mml_err(pos, msg)`、`TrackState.tempo`。
- Produces: `Op::SharedTempo(u32)`、`fn shared_tempo(tracks: &[Vec<Item>]) -> Result<u32, MmlError>`、`run_track(track: u8, items: &[Item], initial_tempo: u32, notes: &mut Vec<NoteEvent>) -> Result<f64, MmlError>`(第 3 引数が増える)。

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾(最後の `}` の直前。`the_largest_v1_loop_still_parses` などの後ろ)に追加する:

```rust
    #[test]
    fn shared_tempo_applies_to_every_track() {
        let s = must_parse("t=60 c4 ; c4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[1].duration_sec, 1.0);
        // 2 本目に書いても、全トラックに効く
        let s = must_parse("c4 ; t=60 c4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[1].duration_sec, 1.0);
        // 書かなければ 120(今までどおり)
        let s = must_parse("c4 ; c4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 0.5);
        approx(must_parse("t=240 c4").notes[0].duration_sec, 0.25);
        approx(must_parse("t=32 c4").notes[0].duration_sec, 60.0 / 32.0);
    }

    #[test]
    fn track_tempo_overrides_the_shared_tempo() {
        // 先頭の `t120`: そのトラックは最初から 120、他のトラックは共有の 60
        let s = must_parse("t=60 t120 c4 ; c4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 1.0);
        // 途中の `t120`: そのトラックは、そこから 120
        let s = must_parse("t=60 c4 t120 c4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[1].start_sec, 1.0);
        approx(s.notes[1].duration_sec, 0.5);
        // `t=` を 2 本目に書いて、1 本目が `t120` で上書きする
        let s = must_parse("t120 c4 ; t=60 c4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 1.0);
    }

    #[test]
    fn shared_tempo_notation_ignores_case_and_spaces() {
        assert_eq!(must_parse("T = 60 C4"), must_parse("t=60 c4"));
        // 他のコマンドの前でも後ろ(最初の音符より前)でも同じ
        assert_eq!(must_parse("o5 l8 t=60 c"), must_parse("t=60 o5 l8 c"));
    }

    #[test]
    fn a_track_with_only_the_shared_tempo_still_counts_as_a_track() {
        let s = must_parse("t=60 ; c4 ; d4");
        assert_eq!(s.track_count, 3);
        assert_eq!(s.notes[0].track, 1);
        approx(s.notes[0].duration_sec, 1.0);
        assert_eq!(must_fail("t=60 ; c ; d ; e ; f").position, 17); // 5 本目の手前の `;`
    }

    #[test]
    fn shared_tempo_combines_with_other_syntax() {
        let s = must_parse("t=60 {c d e}4 ; q50 c4^4");
        assert_eq!(s.notes.len(), 4);
        approx(s.total_sec, 2.0); // 1 本目は 1.0 秒、2 本目は 2.0 秒
        approx(s.notes[3].duration_sec, 1.0); // q50 は全長 2.0 秒の半分
    }

    #[test]
    fn invalid_shared_tempo() {
        assert_eq!(must_fail("t= c").position, 0); // 数字が無い
        assert_eq!(must_fail("t=31 c").position, 2); // 範囲外(数字の位置)
        assert_eq!(must_fail("t=256 c").position, 2);
        assert_eq!(must_fail("c t=140").position, 2); // 音符の後
        assert_eq!(must_fail("r4 t=140 c").position, 3); // 休符の後
        assert_eq!(must_fail("c ; d t=140").position, 6); // 2 本目の音符の後
        assert_eq!(must_fail("[t=140 c]2").position, 1); // 繰り返しの中
        assert_eq!(must_fail("{t=140 c}4").position, 1); // 連符の中
        assert_eq!(must_fail("t=140 t=100 c").position, 6); // 同じトラックに 2 個
        assert_eq!(must_fail("t=140 c ; t=100 d").position, 10); // 別のトラックに 2 個
        assert_eq!(must_fail("t=140").position, 0); // 音符が無い(今までと同じ)
    }

    #[test]
    fn a_later_tracks_syntax_error_comes_before_an_earlier_tracks_run_error() {
        // 全トラックを先に構文解析するので、後ろのトラックの構文エラーが先に出る(エラーが 2 つあるときだけ)
        let e = must_fail("t32 [c1]99 ; x"); // 1 本目は 90 秒を超える
        assert_eq!(e.position, 13);
        assert!(e.message.contains("不明な文字"), "{}", e.message);
        // エラーが 1 つなら、今までどおり実行時のエラー
        let e = must_fail("t32 [c1]99");
        assert!(e.message.contains("90"), "{}", e.message);
    }

    #[test]
    fn four_tracks_at_the_expansion_cap_parse_quickly() {
        // 各トラックが約 49.5 万命令に展開される(上限 50 万の内側)。全トラックを先に構文解析するので、
        // 4 本分の命令が同時にメモリに載る。時間だけを見る(メモリのピークは測っていない)。
        let track = format!("c[[{}]99]10", "><".repeat(250));
        let text = format!("t=120 {track};{track};{track};{track}");
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        let started = std::time::Instant::now();
        let s = must_parse(&text);
        assert_eq!(s.track_count, 4);
        assert_eq!(s.notes.len(), 4);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "parse が {:?} かかった",
            started.elapsed()
        );
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests:: 2>&1 | grep -E "^test .*FAILED|test result"`
Expected: 上で足した 8 つが FAIL(`t=` は今は「不明な文字 `=`」)。ただし `a_later_tracks_syntax_error_comes_before_an_earlier_tracks_run_error` は、今は 1 本目の 90 秒エラーが先に出るので、`position` の検査で FAIL する。既存のテストは PASS のまま。

- [ ] **Step 3: 実装する**

(a) `enum Op` の `Tempo(u32),` の次の行に追加する:

```rust
    /// `t=<n>`。全トラックの初期テンポ(`parse` が集めて `run_track` に渡す。`run_track` では何もしない)。
    SharedTempo(u32),
```

(b) `parse_track` の `'t' => { ... }` アームの**直前**に、次のアームを追加する(match ガード付き。既存の `'t'` アームはそのまま残す):

```rust
            't' if c.peek() == Some('=') => {
                c.bump();
                if !stack.is_empty() {
                    return Err(mml_err(pos, "`t=` は繰り返し・連符の外に書いてください"));
                }
                let has_length = out
                    .iter()
                    .any(|it| matches!(it.op, Op::Note { .. } | Op::Rest { .. } | Op::TieLen { .. }));
                if has_length {
                    return Err(mml_err(
                        pos,
                        "`t=` はトラックの最初の音符・休符より前に書いてください",
                    ));
                }
                let Some((n, p)) = c.number() else {
                    return Err(mml_err(pos, "`t=` の後に数値が必要です"));
                };
                if !(32..=255).contains(&n) {
                    return Err(mml_err(p, "テンポは 32〜255 です"));
                }
                Op::SharedTempo(n)
            }
```

(c) 空白とコメントを除く `fn symbols` の**ドキュメントコメント**(`/// 空白とコメントを除いた (ASCII だけ小文字化した文字, 元の文字位置) の列を作る。` で始まる行)の直前に、次の関数を追加する:

```rust
/// 全トラックの `t=` を集めて、共有の初期テンポを 1 つに決める(書かれていなければ既定の 120)。
/// 2 個目は、その `t` の位置でエラー。
fn shared_tempo(tracks: &[Vec<Item>]) -> Result<u32, MmlError> {
    let mut found: Option<u32> = None;
    for it in tracks.iter().flatten() {
        if let Op::SharedTempo(n) = &it.op {
            if found.is_some() {
                return Err(mml_err(it.pos, "`t=` は 1 回だけ書けます"));
            }
            found = Some(*n);
        }
    }
    Ok(found.unwrap_or(DEFAULT_TEMPO))
}
```

(d) `run_track` を 3 か所変える。

- シグネチャを置き換える。

置き換える前:
```rust
fn run_track(track: u8, items: &[Item], notes: &mut Vec<NoteEvent>) -> Result<f64, MmlError> {
```
置き換えた後:
```rust
fn run_track(
    track: u8,
    items: &[Item],
    initial_tempo: u32,
    notes: &mut Vec<NoteEvent>,
) -> Result<f64, MmlError> {
```

- 初期化の `tempo: DEFAULT_TEMPO,` を `tempo: initial_tempo,` に置き換える(`run_track` の中の `let mut st = TrackState { ... }` にある 1 か所だけ)。
- `match op` の `Op::Tempo(n) => st.tempo = *n,` の次の行に追加する:

```rust
            Op::SharedTempo(_) => {}
```

(e) `parse` の、トラックを 1 本ずつ解析して実行している部分を置き換える。

置き換える前:
```rust
    let mut notes: Vec<NoteEvent> = Vec::new();
    let mut total_sec = 0.0_f64;
    let mut track_count = 0u8;
    for (seg, sep) in segments.into_iter().filter(|(s, _)| !s.is_empty()) {
        if usize::from(track_count) >= MAX_TRACKS {
            return Err(mml_err(sep, "トラックは 4 つまでです"));
        }
        let items = parse_track(seg, total_chars)?;
        let end = run_track(track_count, &items, &mut notes)?;
        total_sec = total_sec.max(end);
        track_count += 1;
    }
```
置き換えた後:
```rust
    // 全トラックを先に構文解析する(`t=` は、どのトラックに書かれていても全トラックの初期テンポになる)
    let mut tracks: Vec<Vec<Item>> = Vec::new();
    for (seg, sep) in segments.into_iter().filter(|(s, _)| !s.is_empty()) {
        if tracks.len() >= MAX_TRACKS {
            return Err(mml_err(sep, "トラックは 4 つまでです"));
        }
        tracks.push(parse_track(seg, total_chars)?);
    }
    let initial_tempo = shared_tempo(&tracks)?;

    let mut notes: Vec<NoteEvent> = Vec::new();
    let mut total_sec = 0.0_f64;
    for (i, items) in tracks.iter().enumerate() {
        let end = run_track(i as u8, items, initial_tempo, &mut notes)?;
        total_sec = total_sec.max(end);
    }
    let track_count = tracks.len() as u8;
```

(`parse` の後ろにある `if notes.is_empty() { ... }` と `Ok(MmlScore { notes, total_sec, track_count })` はそのまま。)

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`(PR #511 までのテストと既存のテスト全部を含む)。`four_tracks_at_the_expansion_cap_parse_quickly` の所要時間(`cargo test --lib mml::tests::four_tracks 2>&1 | grep "finished in"`)を控える。**2 秒を超えたら、先へ進まず、ユーザーに所要時間を報告する**(全トラックを同時に保持する設計の見直し根拠になる)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|generates_frontend_bindings"`
Expected: 失敗なし。`git -C /home/onodai145/repos/github.com/onodai145/tsumugi status --short` に `tauri.gen.ts` が出ない。

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs 2>&1 | grep -c '^Diff in'
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML に全トラックの初期テンポ t= を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
Expected: 差分の数が `5`(増えていない)。増えていたら、追加した部分の整形を直してからコミットする。

---

### Task 2: フロント — `t=` の色分け、「記法」ヘルプ、ユーザーガイド

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlLexer.ts`、`mmlLexer.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/ui/settings/SoundChoicePicker.svelte`、`SoundChoicePicker.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/docs/guide/user-guide.md`

**Interfaces:**
- Consumes: `lexMml(text): MmlToken[]` の `NUMBERED_COMMANDS` と `cmdEnd` の組み立て(`v+` / `v-` と同じ仕組み)、`data-testid="mml-syntax-help"` の `<pre>`。
- Produces: `t=` が 1 つの `command` トークン(`=` まで。空白をまたいでよい)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlLexer.test.ts` の `describe("lexMml", ...)` の**中**(`v+n と v-n は 1 つのコマンド…` の `it` の次)に追加する:

```ts
  it("t= は 1 つのコマンド(= まで。空白をまたいでよい)と number。t120 は今までどおり", () => {
    expect(lex("t=140 t120")).toEqual([
      ["t=", "command"],
      ["140", "number"],
      ["t", "command"],
      ["120", "number"],
    ]);
    expect(lex("T = 140")).toEqual([
      ["T =", "command"],
      ["140", "number"],
    ]);
  });
```

`frontend/src/ui/settings/SoundChoicePicker.test.ts` の「記法ヘルプに、タイ ^・相対音量…」の `it` の次に追加する:

```ts
  it("記法ヘルプに、全トラックの初期テンポ t= の書き方が出る", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    expect(text).toContain("t=140");
    expect(text).toContain("全トラックの初期テンポ");
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | grep -E "×|Test Files|Tests |Error"`
Expected: 新しい 2 つの `it` が FAIL(`t=` は今は `t` と、`=` が `unknown`。ヘルプに `t=140` が無い)。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlLexer.ts` の `cmdEnd` を置き換える(`v+` / `v-` に、`t=` を足す):

置き換える前:
```ts
      // `v+2` `v-2` は、符号までを 1 つのコマンドにする
      const cmdEnd = i + (ch === "v" && (at(i + 1) === "+" || at(i + 1) === "-") ? 2 : 1);
```
置き換えた後:
```ts
      // `v+2` `v-2` は符号まで、`t=140` は `=` までを 1 つのコマンドにする
      const signed = ch === "v" && (at(i + 1) === "+" || at(i + 1) === "-");
      const shared = ch === "t" && at(i + 1) === "=";
      const cmdEnd = i + (signed || shared ? 2 : 1);
```

(b) `frontend/src/ui/settings/SoundChoicePicker.svelte` の「記法」の `<pre>` の、`l8 既定の長さ  t120 テンポ …` の行の**次の行**に追加する:

```
t=140 全トラックの初期テンポ(最初の音符より前に 1 回だけ。トラックごとの t で上書き)
```

(c) `docs/guide/user-guide.md` を編集する(Python などで、置き換え前の文字列がちょうど 1 回出ることを確かめながら置き換える)。

1. 記法表の `t120` の行の次に、次の 1 行を足す:

```markdown
| `t=140` | 全トラックの初期テンポ（32〜255）。トラックの最初の音符・休符より前に、1 回だけ書ける（どのトラックでもよい。繰り返し・連符の中は不可）。トラックごとの `t120` は、そのトラックだけを上書きする。`t=140` だけのトラックもトラックに数えられる（上限の 4 に含まれ、音量が下がる）ので、普通は最初のトラックの頭に書く（`t=140 c d e ; o3 c g`） |
```

2. 説明の段落の文を置き換える:

置き換える前:
```
テンポを揃えたいときは各トラックに `t` を書いてください。
```
置き換えた後:
```
テンポを揃えたいときは、最初のトラックの頭に `t=140` を書くか、各トラックに `t` を書いてください。
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts src/ui/settings/SoundChoicePicker.test.ts src/ui/settings/mmlEditorExtensions.test.ts 2>&1 | grep -E "×|Test Files|Tests |Error"`
Expected: すべて PASS。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -4`
Expected: 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件だけは出てよい)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md && git commit -m "feat: MML の t= を色分けし、ヘルプとガイドに追記(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 全体の確認と実機の確認

**Files:**
- 変更なし(確認だけ。直す必要が出たら、該当タスクの続きとして直す)。確認の記録を spec の末尾に追記する場合のみ、`docs/superpowers/specs/2026-10-20-issue-499-mml-shared-tempo-design.md` を編集する。

- [ ] **Step 1: 自動テストをすべて走らせる**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|panicked" | head
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -3
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm test 2>&1 | grep -E "Test Files|Tests "
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short
```
Expected: `cargo test` は失敗 0(`#[ignore]` は除く)。`pnpm check` は 0 errors。`pnpm test` は全件 PASS。`git status` が空(`tauri.gen.ts` が変わっていない)。

- [ ] **Step 2: 実アプリで確認する(隔離した Xvfb のインスタンス)**

実画面に出さず、ユーザーの本物のデータ(`~/.cache/com.onodai.tsumugi/cache.db` など)に触れない。手順は、メモリの次の 3 件を**先に読んでから**従う:
- `feedback-dev-server-verification-must-use-virtual-display.md`(Xvfb 越し。`WAYLAND_DISPLAY` も unset)
- `tauri-single-instance-blocks-xdg-isolated-verification.md`(`dbus-run-session` が要る)
- `reference-isolated-instance-needs-short-xdg-path.md`(隔離インスタンスの bridge のパスは `/tmp` 直下の短いパス)

併せて、`CLAUDE.md` の「debug_bridge」の節(`curl --unix-socket … --data-binary '<js>'`、`return` が要る、JSON を 2 回デコードする)と、メモリ `debug-bridge-does-not-support-await.md`(`await` は使えない)に従う。PR #511 の確認(spec `2026-10-20-issue-499-mml-syntax-extension-design.md` の「実装後の確認」)と同じ方法でよい:
- 起動: `Xvfb :<空き番号>`、`dbus-run-session`、`XDG_CACHE_HOME` / `XDG_DATA_HOME` / `XDG_CONFIG_HOME` を `/tmp/<短い名前>` 配下に、`WAYLAND_DISPLAY` は unset、`GDK_BACKEND=x11`、`target/debug/tsumugi` を直接起動(ユーザーの `cargo tauri dev` の vite(127.0.0.1:5173)が動いていて作業ツリーを配信していれば、`cargo build` も `pnpm build` も要らない。動いていなければ、`cd frontend && pnpm build` のあとに `cd src-tauri && cargo build`)。起動した全プロセスに、一意なトークンを環境変数で付ける。
- アカウント: 起動前に `$XDG_CONFIG_HOME/com.onodai.tsumugi/settings.json` へ、ダミーのアカウント(`{"accounts":[{"id":"00000000-0000-4000-8000-000000000499","host":"misskey.invalid","username":"verify","userId":"verifyuser","displayName":"verify","avatarUrl":null}]}`)だけを書く。
- 画面: 投稿欄の「メニュー」→「設定」→「通知」→「通知音を鳴らす」ON → 種類「MML（打ち込み）」。入力は `xdotool type`(エディタに `.focus()` したあと)。画面は `import -window root` で撮って自分で見る。

確認すること(それぞれ、画面のスクリーンショットか bridge の値を自分で見る):
1. `t=60 c4 d4 ; e4 f4` で、構文エラーが出ず、「再生時間 2.0 秒」・トラック 2/4。`t=` が色分けされる(コマンドの色)。
2. 同じ MML から `t=60 ` を消した `c4 d4 ; e4 f4` で、「再生時間 1.0 秒」(共有を書かなければ今までどおり 120)。
3. `c4 t=140`: 位置つきのエラー(「3 文字目: `t=` はトラックの最初の音符・休符より前に書いてください」)で、試聴と保存が `disabled`。「3 文字目」を押すと、カーソルが `t` に移る。
4. `[t=140 c]2` と `t=140 c ; t=100 d`(「11 文字目: `t=` は 1 回だけ書けます」)で、それぞれ位置つきのエラー。
5. 「記法」を開くと、`t=140 全トラックの初期テンポ…` の行が出る。
6. 手順 1 の MML で「試聴」を押し、`play_notify_sound` に `data:audio/wav;base64,…` が渡る(bridge で `fetch` の `ipc://` を包んで記録する。`window.__TAURI_INTERNALS__.invoke` は書き換えられない)。**音が鳴ること自体は、Xvfb に音声デバイスが無いので確認できない**。そう書く。

確認が終わったら、自分で起動した Xvfb / `dbus-run-session` / アプリ / bridge のプロセスを、トークンで特定した PID を指定して止める(`pkill` / `killall` は使わない)。`/tmp` に作った隔離ホームと、Xvfb のロックファイルも片付ける。ユーザー自身の `cargo tauri dev` のプロセスには触れない。

- [ ] **Step 3: 結果を spec の末尾に記録する**

`docs/superpowers/specs/2026-10-20-issue-499-mml-shared-tempo-design.md` の末尾に、次の節を追記する(実際に確認したことと、確認できなかったことを、事実のとおりに書く。数値と所要時間はこの手順で得た実測値を入れる。括弧書きのまま残さない):

```markdown
## 実装後の確認

(確認した日付・ブランチ・HEAD のコミット、隔離インスタンスの起動方法を書く。)

確認できたこと:
- 自動テスト: `cargo test` の結果、`pnpm check` の結果、`pnpm test` の結果(件数)。
- `rustfmt --check src/mml.rs` の差分の数(`main` と同じ 5 かどうか)。
- 4 トラックが展開上限に近い入力(`four_tracks_at_the_expansion_cap_parse_quickly`)の所要時間。
- 実アプリ(隔離インスタンス): Step 2 の 1〜6 のそれぞれで、何を見て、何が起きたか。

確認できなかったこと:
- 実際に音が鳴ること、聞こえ方(Xvfb に音声デバイスが無い)。
- 4 トラックが展開上限に近いときのメモリのピーク(時間だけを測った)。
- Android の実機・エミュレーター、macOS / Windows。
```

- [ ] **Step 4: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add docs/superpowers/specs/2026-10-20-issue-499-mml-shared-tempo-design.md && git commit -m "docs: MML のテンポ共有の実装後の確認を記録(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: ブランチの扱いを決める(ユーザーに確認する)**

`superpowers:finishing-a-development-branch` に従う。PR を作る場合は、`.github/pull_request_template.md` の構成に手で合わせ、本文に `Refs #499` と書く(`Fixes` / `Closes` は書かない。#499 の「中くらい」の候補が未着手で残るため)。`gh pr merge` は `--merge`(squash にしない)。push 後に CI を待つループは作らない(ユーザーが見る)。#499 のチェックボックス(「トラック間のテンポ共有」)を更新するかは、ユーザーに確認してから行う。作業ブランチの削除も、ユーザーが頼んだときだけ行う。
