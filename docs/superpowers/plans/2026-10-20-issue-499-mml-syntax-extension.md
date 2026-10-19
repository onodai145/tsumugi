# MML 構文の拡張(タイ・連符・コメントなど) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML の方言に、`^` タイ・相対音量 `v+n`/`v-n`・コメント(`//` と `/* */`)・連符 `{…}n`・ループの脱出 `:` とネスト・ゲートタイム `q` を足し、字句解析・入力補助キー・ヘルプ・ガイドを追随させる。

**Architecture:** 6 項目とも `src-tauri/src/mml.rs` の中で、今までと同じ `NoteEvent` の列に落とし込む(`NoteEvent` / `MmlScore` / レンダラーは変えない)。コメントは `parse` の入口で、空白を落とす前に取り除く。ループと連符の「開き」は 1 つのスタックで管理する。連符の縮小率は `run_track` が先読みで求める。フロントは、コメントの範囲を求める純関数 `mmlComments.ts` を字句解析と使用量の数え方が共有する。

**Tech Stack:** Rust(`src-tauri`、`cargo test`)、Svelte 5 + TypeScript(`frontend`、Vitest + jsdom)、CodeMirror 6。

**Spec:** `docs/superpowers/specs/2026-10-20-issue-499-mml-syntax-extension-design.md`(前提: `2026-10-15-issue-75-mml-ringtone-design.md` / `2026-10-16-issue-494-mml-timbre-design.md` / `2026-10-17-issue-500-mml-editor-design.md`)

## Global Constraints

- 作業ブランチは `feat/issue-499-mml-syntax-extension`(作成済み)。`main` には触らない。リポジトリルートは `/home/onodai145/repos/github.com/onodai145/tsumugi`。コマンドは絶対パスで書く(作業ディレクトリが変わっても動くように)。
- v1 の記法・既定値・範囲は変えない。今まで通っていた MML は、同じ `MmlScore` になる(新しい記号 `^` `{` `}` `:` `/` `q` と `v+` `v-` は今は構文エラーなので、保存済みの MML の意味は変わらない)。
- `NoteEvent` / `MmlScore` / `MmlError` / `Timbre` と `mmlRender.ts` は変えない。`frontend/src/bindings/tauri.gen.ts` は変わらない(変わったら何かがおかしい)。
- 上限は 5000 文字(コメントも含む)・3000 音・4 トラック・90 秒。値は変えない。新設: 展開後の命令数 `MAX_EXPANDED_OPS = 500_000`(Rust のみ。フロントには持たない)。
- エラー位置は元のテキストの文字位置(Unicode スカラ値の添字、0 始まり、空白とコメントを含む)。UI は 1 始まりの「N 文字目」で出す。新しい記法の誤りも、すべて `MmlError { position, message }` で返す。
- `q` は百分率 1〜100、既定 100。`v+n`/`v-n` の `n` は 1〜15(省略は 1)で、結果が 0〜15 を出たらエラー(丸めない)。連符は入れ子禁止で、長さの比で割る。`{` の後の長さの省略は「`{` の時点の `l`」。ループの `:` は 1 つのループに 1 個まで、最後の回だけ `:` 以降を飛ばす。
- 新しい UI の値は `docs/design/style-guide.md` に沿う(`rounded-[Npx]` などの即値を作らない)。新しいテーマトークンは作らず、既存トークンの `color-mix` で足りる範囲に留める。
- コミットメッセージは件名のみ(本文・箇条書きなし)。ハーネスの指示どおり、末尾に `Co-Authored-By` トレーラーを付ける(下の例は `-m` を 2 回に分けている)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら、止めてユーザーに報告する(再試行しない)。
- Rust のテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::<名前>`。単一のフロントテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run <パス>`(`pnpm test -- <パス>` はパスを無視して全件走る)。
- `mml.rs` は `rustfmt --check --edition 2021` が通る状態を保つ(リポジトリには、他のファイルに既存の整形差分がある。`cargo fmt` を全体に掛けない)。
- PR を作るときは、本文に `Fixes #499` / `Closes #499` を書かない(#499 には「中くらい」の候補が未着手で残る)。`Refs #499` と書く。Issue のチェックボックスを更新するのは、ユーザーの確認を取ってから。
- `cargo tauri dev` / 実アプリを起動した確認では、起動したプロセスを自分で PID 指定で止める(`pkill` / `killall` は使わない)。

## Review Focus

仕様が黙っているが、使う人が踏みやすい入力と条件。それぞれ、持ち主のタスクにテストを足してある。

1. ネストしたループの展開が爆発する入力(`[[[[c]99]99]99]99`)。メモリを使い切らず、短時間で「命令が多すぎる」エラーを返す。v1 で通る最大のループ(約 49.5 万命令)は、今までどおり通る。(Task 5)
2. コメントの中の `;` `/*` `//`、閉じ忘れの `/*`、コメントの直後のエラー位置、CRLF、絵文字(サロゲートペア)。コメントでトラックが分かれず、位置が元のテキストのままである。(Task 1、Task 7)
3. 記法どうしの組み合わせ。タイ・`q`・連符(`q50 c4&c4`、`{c4^4 d4}2`)、`c4&^8`(`&` の直後の `^`)、ループの中の連符と連符の中のループ。(Task 2、4、6)
4. 連符の例外的な中身。空の連符、音符のない連符(`{o5}4`)、中での `l` と `t` の変更、`{ [ } ]` のような括弧の交差。(Task 6)
5. v1 の MML が同じ結果になること(既存テストがそのまま通る。`^` などを含まない MML で、音の高さ・長さ・開始時刻が変わらない)。(Task 1 の回帰テストと、全タスクの `cargo test --lib mml::`)
6. フロント: 閉じていない `/*` が以降をすべてコメント色にすること、`//` が改行で終わること、コメントの中の `;` をトラックとして数えないこと、`v+2` が 1 つのコマンドとして色分けされること。(Task 7)

---

### Task 1: コメント(`//` と `/* */`)を `parse` の入口で取り除く

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(ファイル先頭のコメント、`parse` の直前に `symbols` を追加、`parse` の `syms` の作り方、`mod tests` にテスト)

**Interfaces:**
- Produces: `fn symbols(text: &str) -> Result<Vec<(char, usize)>, MmlError>` — 空白とコメントを除いた「(ASCII だけ小文字化した文字, 元の文字位置)」の列。閉じていない `/*` は `/*` の位置の `MmlError`。以降のタスクは `parse` 経由でしか使わない。

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾(`validate_sound_choice_checks_only_mml` の後、`timbres` ヘルパーの前でもよい。最後の `}` の直前)に追加する:

```rust
    /// v1 の記法は、v2 でも同じ結果になる(新しい記号は v1 では構文エラーだったので、意味は変わらない)。
    #[test]
    fn v1_syntax_keeps_its_results() {
        let s = must_parse("t120 o5 l8 cdefgab>c");
        let midis: Vec<u8> = s.notes.iter().map(|n| n.midi).collect();
        assert_eq!(midis, vec![72, 74, 76, 77, 79, 81, 83, 84]);
        for (i, n) in s.notes.iter().enumerate() {
            approx(n.start_sec, 0.25 * i as f64);
            approx(n.duration_sec, 0.25);
        }
        approx(s.total_sec, 2.0);
    }

    #[test]
    fn line_comments_run_to_the_end_of_the_line() {
        assert_eq!(must_parse("c4 // d4 e4\nf4"), must_parse("c4 f4"));
        assert_eq!(must_parse("c4 // 末尾までコメント"), must_parse("c4"));
        assert_eq!(must_parse("// 全部コメント\nc"), must_parse("c"));
        assert_eq!(must_parse("c4 // x\r\nd4"), must_parse("c4 d4")); // CRLF
    }

    #[test]
    fn block_comments_may_span_lines_and_do_not_nest() {
        assert_eq!(must_parse("c4 /* d4 */ e4"), must_parse("c4 e4"));
        assert_eq!(must_parse("c4/*複数行\nの*/e4"), must_parse("c4e4"));
        assert_eq!(must_parse("c/**/d"), must_parse("cd"));
        // 入れ子はできない: 最初の `*/` で閉じ、残りの ` c */ d` は音符 c と、不明な文字 `*`
        assert_eq!(must_fail("/* a /* b */ c */ d").position, 15);
    }

    #[test]
    fn unterminated_block_comment_is_an_error_at_its_opening() {
        assert_eq!(must_fail("c4 /* d4").position, 3);
        assert_eq!(must_fail("c /*/").position, 2); // `/*/` は閉じていない
    }

    #[test]
    fn a_lone_slash_is_still_an_unknown_character() {
        assert_eq!(must_fail("c/d").position, 1);
        assert_eq!(must_fail("c / / d").position, 2); // 間に空白があれば `//` ではない
    }

    #[test]
    fn comments_do_not_split_tracks() {
        assert_eq!(must_parse("c4 // ; d4\ne4").track_count, 1);
        assert_eq!(must_parse("c4 /* ; */ e4").track_count, 1);
        assert_eq!(must_parse("c4 /* x */ ; e4").track_count, 2);
    }

    #[test]
    fn error_positions_after_comments_stay_in_the_original_text() {
        // `/* あ */ c4 x` で x は 12 文字目(0 始まりの 11)
        assert_eq!(must_fail("/* あ */ c4 x").position, 11);
        assert_eq!(must_fail("// あ\nx").position, 5);
    }

    #[test]
    fn text_length_limit_counts_comments() {
        // `c //` が 4 文字、残り 4996 文字がコメント: ちょうど 5000 文字
        assert!(parse(&format!("c //{}", "あ".repeat(4996))).is_ok());
        let e = must_fail(&format!("c //{}", "あ".repeat(4997)));
        assert_eq!(e.position, 5000);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests:: 2>&1 | tail -30`
Expected: `v1_syntax_keeps_its_results` は PASS(v1 の挙動の固定)、コメントを扱う 6 つは FAIL(`/` が不明な文字になる)。

- [ ] **Step 3: 実装する**

(a) ファイル先頭の `//!` コメントの最後の行を差し替える:

```rust
//! 方言と上限は docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md、
//! 音色は 2026-10-16-issue-494-mml-timbre-design.md、
//! 構文の拡張(タイ・連符・コメントなど)は 2026-10-20-issue-499-mml-syntax-extension-design.md を参照。
```

(既存の `//! 方言と上限は docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md を参照。` の 1 行を上の 3 行に置き換える。)

(b) `/// MML テキストを解釈する。` の直前に、次の関数を追加する:

```rust
/// 空白とコメントを除いた (ASCII だけ小文字化した文字, 元の文字位置) の列を作る。
/// コメントは `// …`(改行の手前まで)と `/* … */`(入れ子にできない)。`/` だけはコメントにならず、
/// そのまま列に残って `parse_track` が「不明な文字」にする。`;` はトラック区切りなのでコメントの開始にしない。
/// 閉じていない `/*` は、その `/*` の位置のエラー。
fn symbols(text: &str) -> Result<Vec<(char, usize)>, MmlError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let open = i;
            i += 2;
            loop {
                if i + 1 >= chars.len() {
                    return Err(mml_err(open, "`/*` が閉じていません"));
                }
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if !c.is_whitespace() {
            out.push((c.to_ascii_lowercase(), i));
        }
        i += 1;
    }
    Ok(out)
}
```

(c) `parse` の中の、次のブロックを置き換える。

置き換える前:

```rust
    let syms: Vec<(char, usize)> = text
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(i, c)| (c.to_ascii_lowercase(), i))
        .collect();
```

置き換えた後:

```rust
    let syms = symbols(text)?;
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`(既存のテストも全部通る)。

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs && echo fmt-ok
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML にコメント(// と /* */)を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `^` タイ(長さを足す)

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(`Op`、`parse_track`、`run_track`、tests)

**Interfaces:**
- Consumes: `Cursor::length_and_dots() -> Result<(Option<u32>, u8), MmlError>`、`duration_sec(len: u32, dots: u8, tempo: u32) -> f64`(この時点の 3 引数版)、`TrackState.last_note` / `tie_pos`。
- Produces: `Op::TieLen { len: Option<u32>, dots: u8 }`。

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾に追加する:

```rust
    #[test]
    fn caret_extends_the_previous_note() {
        let s = must_parse("c4^8");
        assert_eq!(s.notes.len(), 1);
        approx(s.notes[0].duration_sec, 0.75);
        approx(must_parse("c4^").notes[0].duration_sec, 1.0); // 長さ省略は l(4)
        approx(must_parse("l8 c^").notes[0].duration_sec, 0.5);
        approx(must_parse("c4^8.").notes[0].duration_sec, 0.5 + 0.375);
        approx(must_parse("c4^4^4").notes[0].duration_sec, 1.5);
        approx(must_parse("c4&c8^8").notes[0].duration_sec, 1.0); // `&` との併用
        approx(must_parse("c4^8&c8").notes[0].duration_sec, 1.0);
        let s = must_parse("c4^8 d4");
        approx(s.notes[1].start_sec, 0.75);
        assert_eq!(must_parse("[c4^8]2").notes.len(), 2);
        assert_eq!(must_parse("C4^8"), must_parse("c4 ^ 8"));
    }

    #[test]
    fn invalid_carets() {
        assert_eq!(must_fail("^4 c").position, 0); // 前に音符が無い
        assert_eq!(must_fail("r4^4 c").position, 2); // 休符の後ろ
        assert_eq!(must_fail("c4 r4 ^4").position, 6);
        assert_eq!(must_fail("c4&^8").position, 3); // `&` の直後
        assert_eq!(must_fail("c4^3").position, 3); // 長さが不正
        assert_eq!(must_fail("c4^8...").position, 6); // 3 個目の付点
        assert_eq!(must_fail("c4^8&").position, 4); // 後ろに音符が無い `&`
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests:: 2>&1 | grep -E "caret|FAILED|test result"`
Expected: 2 つとも FAIL(`^` が不明な文字)。

- [ ] **Step 3: 実装する**

(a) `enum Op` の `Tie,` の次の行に追加する:

```rust
    /// `^`。直前の音符に、この長さを足す(長さ省略は `l` の値)。
    TieLen {
        len: Option<u32>,
        dots: u8,
    },
```

(b) `parse_track` の `'&' => Op::Tie,` の次に追加する:

```rust
            '^' => {
                let (len, dots) = c.length_and_dots()?;
                Op::TieLen { len, dots }
            }
```

(c) `run_track` の `Op::Tie => { ... }` のアームの次に追加する:

```rust
            Op::TieLen { len, dots } => {
                if st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の直後には `^` を置けません"));
                }
                let Some(idx) = st.last_note else {
                    return Err(mml_err(pos, "`^` の前に音符が必要です"));
                };
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo);
                notes[idx].duration_sec += d;
                st.t += d;
                check_limit(st.t, pos)?;
            }
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs && echo fmt-ok
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML に ^ タイ(長さを足す)を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 相対音量 `v+n` / `v-n`

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(`Op`、`parse_track` の `'v'`、`run_track`、tests)

**Interfaces:**
- Produces: `Op::VolumeDelta(i32)`。

- [ ] **Step 1: 失敗するテストを書く**

```rust
    #[test]
    fn relative_volume() {
        let vel = |t: &str| must_parse(t).notes[0].velocity;
        approx(vel("v5 v+2 c"), 7.0 / 15.0);
        approx(vel("v5 v-2 c"), 3.0 / 15.0);
        approx(vel("v5 v+ c"), 6.0 / 15.0); // 数字省略は 1
        approx(vel("v5 v- c"), 4.0 / 15.0);
        approx(vel("v+5 c"), 1.0); // 既定 v10 から 15
        approx(vel("v-10 c"), 0.0);
        approx(vel("V5 V + 2 c"), 7.0 / 15.0); // 大文字・空白
        // 絶対指定の v は今までどおり
        approx(vel("v15 c"), 1.0);
    }

    #[test]
    fn relative_volume_errors() {
        assert_eq!(must_fail("v+6 c").position, 0); // 10 + 6 = 16 は範囲外。`v` の位置
        assert_eq!(must_fail("v0 v- c").position, 3); // 0 - 1 は範囲外
        assert_eq!(must_fail("v+0 c").position, 2); // n は 1〜15。数字の位置
        assert_eq!(must_fail("v+16 c").position, 2);
        assert_eq!(must_fail("v c").position, 0); // 絶対指定は数字が必要なまま
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::relative_volume 2>&1 | tail -20`
Expected: FAIL(`v+` で「`v` の後に数値が必要です」)。

- [ ] **Step 3: 実装する**

(a) `enum Op` の `Volume(u8),` の次に追加する:

```rust
    /// `v+n` / `v-n`。現在の音量に足す(範囲外はエラー)。
    VolumeDelta(i32),
```

(b) `parse_track` の `'v' => { ... }` アームを置き換える。

置き換える前:

```rust
            'v' => {
                let (n, p) = c.require_number(pos, 'v')?;
                if n > 15 {
                    return Err(mml_err(p, "音量は 0〜15 です"));
                }
                Op::Volume(n as u8)
            }
```

置き換えた後:

```rust
            'v' => {
                if let Some(sign @ ('+' | '-')) = c.peek() {
                    c.bump();
                    let n = match c.number() {
                        Some((n, p)) => {
                            if !(1..=15).contains(&n) {
                                return Err(mml_err(p, "相対音量は 1〜15 です"));
                            }
                            n
                        }
                        None => 1,
                    };
                    Op::VolumeDelta(if sign == '+' { n as i32 } else { -(n as i32) })
                } else {
                    let (n, p) = c.require_number(pos, 'v')?;
                    if n > 15 {
                        return Err(mml_err(p, "音量は 0〜15 です"));
                    }
                    Op::Volume(n as u8)
                }
            }
```

(c) `run_track` の `Op::Volume(n) => st.volume = u32::from(*n),` の次に追加する:

```rust
            Op::VolumeDelta(delta) => {
                let v = st.volume as i32 + delta;
                if !(0..=15).contains(&v) {
                    return Err(mml_err(pos, "音量は 0〜15 です"));
                }
                st.volume = v as u32;
            }
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs && echo fmt-ok
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML に相対音量 v+n / v-n を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: ゲートタイム `q`

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(定数、`Op`、`parse_track`、`TrackState`、`run_track`、tests)

**Interfaces:**
- Consumes: Task 2 の `Op::TieLen` の処理(全長の持ち方を変えるので、ここで書き換える)。
- Produces: `Op::Gate(u8)`、`const MAX_GATE: u32 = 100`、`fn gated(full: f64, gate: u32) -> f64`、`TrackState` の `gate` / `last_full` / `last_gate`。

- [ ] **Step 1: 失敗するテストを書く**

```rust
    #[test]
    fn gate_shortens_notes_but_not_the_timeline() {
        let s = must_parse("q50 c4 d4");
        approx(s.notes[0].duration_sec, 0.25);
        approx(s.notes[1].start_sec, 0.5); // 次の音の開始は変わらない
        approx(s.notes[1].duration_sec, 0.25);
        approx(s.total_sec, 1.0); // 再生時間は全長で数える
        approx(must_parse("c4 q50 d4").notes[0].duration_sec, 0.5); // 変更前の音は影響を受けない
        approx(must_parse("q100 c4").notes[0].duration_sec, 0.5);
        approx(must_parse("q1 c4").notes[0].duration_sec, 0.005);
        // 休符は q の影響を受けない(そのまま進む)
        approx(must_parse("q50 c4 r4 d4").notes[1].start_sec, 1.0);
    }

    #[test]
    fn gate_applies_to_the_whole_tied_note() {
        approx(must_parse("q50 c4&c4").notes[0].duration_sec, 0.5); // 全長 1.0 の半分
        approx(must_parse("q50 c4^4").notes[0].duration_sec, 0.5);
        // タイの途中で q を変えても、先頭の音の q を使う
        approx(must_parse("q50 c4&q100 c4").notes[0].duration_sec, 0.5);
        approx(must_parse("q50 c4^4&c4").notes[0].duration_sec, 0.75);
    }

    #[test]
    fn gate_is_per_track_and_validated() {
        approx(must_parse("q50 c4;c4").notes[1].duration_sec, 0.5);
        assert_eq!(must_fail("q0 c").position, 1);
        assert_eq!(must_fail("q101 c").position, 1);
        assert_eq!(must_fail("q c").position, 0);
    }

    #[test]
    fn default_gate_leaves_durations_bit_exact() {
        // q100 は掛け算をしないので、q を書かない場合と、浮動小数点の値まで完全に同じになる
        assert_eq!(
            must_parse("q100 t137 c4.").notes[0].duration_sec,
            must_parse("t137 c4.").notes[0].duration_sec
        );
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::gate 2>&1 | tail -20`
Expected: FAIL(`q` が不明な文字)。

- [ ] **Step 3: 実装する**

(a) 定数を追加する(`const DEFAULT_VOLUME: u32 = 10;` の次):

```rust
/// ゲートタイム `q` の範囲(百分率)。既定は上限の 100(全長を鳴らす)。
const MAX_GATE: u32 = 100;
```

(b) `enum Op` の `VolumeDelta(i32),` の次に追加する:

```rust
    /// `q<n>`。音を鳴らす長さの割合(百分率、1〜100)。
    Gate(u8),
```

(c) `parse_track` の `'@' => {` アームの直前に追加する:

```rust
            'q' => {
                let (n, p) = c.require_number(pos, 'q')?;
                if !(1..=MAX_GATE).contains(&n) {
                    return Err(mml_err(p, format!("ゲートタイムは 1〜{MAX_GATE} です")));
                }
                Op::Gate(n as u8)
            }
```

(d) `struct TrackState` を置き換える。

置き換える前:

```rust
struct TrackState {
    octave: i32,
    len: u32,
    tempo: u32,
    volume: u32,
    timbre: Timbre,
    t: f64,
```

置き換えた後(`last_note` 以降のフィールドはそのまま残し、`last_note` の次に 2 つ足す):

```rust
struct TrackState {
    octave: i32,
    len: u32,
    tempo: u32,
    volume: u32,
    gate: u32,
    timbre: Timbre,
    t: f64,
```

`last_note: Option<usize>,` の行の次に追加する:

```rust
    /// 直前の音符の全長(秒。ゲートを掛ける前。タイで延びる)
    last_full: f64,
    /// 直前の音符を作ったときのゲート(タイで延びても、先頭の音符のものを使う)
    last_gate: u32,
```

(e) `duration_sec` の次に追加する:

```rust
/// 全長にゲートタイム(百分率)を掛ける。100 のときは掛け算をせず、v1 と同じ値のまま返す。
fn gated(full: f64, gate: u32) -> f64 {
    if gate >= MAX_GATE {
        full
    } else {
        full * f64::from(gate) / f64::from(MAX_GATE)
    }
}
```

(f) `run_track` の中:

- `let mut st = TrackState {` の初期化に `gate: MAX_GATE,`(`volume: DEFAULT_VOLUME,` の次)と `last_full: 0.0, last_gate: MAX_GATE,`(`last_note: None,` の次)を足す。
- `Op::Volume` 系のアームの次に `Op::Gate(n) => st.gate = u32::from(*n),` を足す。
- `Op::TieLen` のアームの `notes[idx].duration_sec += d;` を次の 2 行に置き換える:

```rust
                st.last_full += d;
                notes[idx].duration_sec = gated(st.last_full, st.last_gate);
```

- `Op::Note` のアームの、タイの連結(`notes[idx].duration_sec += d;`)を次の 2 行に置き換える:

```rust
                    st.last_full += d;
                    notes[idx].duration_sec = gated(st.last_full, st.last_gate);
```

- `Op::Note` のアームの、新しい音符を積む部分を置き換える。

置き換える前:

```rust
                    notes.push(NoteEvent {
                        track,
                        start_sec: st.t,
                        duration_sec: d,
                        midi: midi as u8,
                        velocity: f64::from(st.volume) / 15.0,
                        timbre: st.timbre,
                    });
                    st.last_note = Some(notes.len() - 1);
```

置き換えた後:

```rust
                    notes.push(NoteEvent {
                        track,
                        start_sec: st.t,
                        duration_sec: gated(d, st.gate),
                        midi: midi as u8,
                        velocity: f64::from(st.volume) / 15.0,
                        timbre: st.timbre,
                    });
                    st.last_note = Some(notes.len() - 1);
                    st.last_full = d;
                    st.last_gate = st.gate;
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`(既存のテストの `duration_sec` の厳密比較 `assert_eq!(a, b)` も通る)。

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs && echo fmt-ok
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML にゲートタイム q を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: ループの脱出 `:` とネスト、展開後の命令数の上限

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(定数、`LoopFrame` と `extend_checked` の追加、`parse_track` の `open_loop` を置き換え、`parse_track` のコメント、tests の `invalid_loops`)

**Interfaces:**
- Produces: `pub const MAX_EXPANDED_OPS: usize = 500_000`、`struct LoopFrame { start, pos, colon }`、`fn extend_checked(out: &mut Vec<Item>, part: &[Item], pos: usize) -> Result<(), MmlError>`。Task 6 が `LoopFrame` を `enum Frame` に置き換える。

- [ ] **Step 1: 失敗するテストを書く**

まず既存の `invalid_loops` から、ネスト禁止を確かめる 1 行を**削除**する(ネストは許可になる):

```rust
        assert_eq!(must_fail("[[c]2]2").position, 1);
```

次に、`mod tests` の末尾に追加する:

```rust
    fn midis(t: &str) -> Vec<u8> {
        must_parse(t).notes.iter().map(|n| n.midi).collect()
    }

    #[test]
    fn loop_escape_skips_the_tail_on_the_last_round() {
        assert_eq!(midis("[c:d]3"), vec![60, 62, 60, 62, 60]);
        assert_eq!(midis("[c:d]"), vec![60, 62, 60]); // 回数省略は 2
        assert_eq!(midis("[:c]2"), vec![60]);
        assert_eq!(midis("[c:]2"), vec![60, 60]);
        assert_eq!(midis("[cd:e]4"), vec![60, 62, 64, 60, 62, 64, 60, 62, 64, 60, 62]);
    }

    #[test]
    fn loops_can_be_nested() {
        assert_eq!(midis("[c[d]2]2"), vec![60, 62, 62, 60, 62, 62]);
        assert_eq!(midis("[[c]2 d]2"), vec![60, 60, 62, 60, 60, 62]);
        // 内側のループの `:` は内側に効く
        assert_eq!(midis("[c[d:e]2]2"), vec![60, 62, 64, 62, 60, 62, 64, 62]);
        // 外側の `:` は、内側のループを含めた手前までが「最後の回にも鳴る部分」
        assert_eq!(midis("[a[d]2:c]2"), vec![69, 62, 62, 60, 69, 62, 62]);
    }

    #[test]
    fn invalid_loop_escapes() {
        assert_eq!(must_fail("c:d").position, 1); // ループの外
        assert_eq!(must_fail("[c:d:e]2").position, 4); // 2 個目
        assert_eq!(must_fail("[c:d]1").position, 5); // 回数は 2〜99 のまま
        assert_eq!(must_fail("[c;d]2").position, 0); // `;` は繰り返しをまたげない
    }

    #[test]
    fn nested_loop_expansion_is_capped_before_it_explodes() {
        // 4 重のネスト: 99^4 個に展開されてしまうが、50 万を超えた `]` で止まる(メモリを使い切らない)。
        // 3 つ目の `]`(位置 11)の展開が 9801 × 99 = 970,299 個になるところで止まる。
        let e = must_fail("[[[[c]99]99]99]99");
        assert_eq!(e.position, 11);
        assert!(e.message.contains("命令"), "{}", e.message);
    }

    #[test]
    fn the_largest_v1_loop_still_parses() {
        // v1 で通る最大級: ループ 1 組で、本体が 4994 文字、99 回の繰り返しで約 49.4 万命令。
        // `><` はオクターブを往復するだけで音を出さない(音の上限・時間の上限では抑えられない命令)。
        let text = format!("c[{}]99", "><".repeat(2497));
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        assert_eq!(must_parse(&text).notes.len(), 1);
        // ネストで 50 万を超える: 内側が 198,000 命令、外側の 2 回目の複製で 594,000 になる。
        let body = "><".repeat(1000);
        let text = format!("c[[{body}]99]99");
        let e = must_fail(&text);
        assert_eq!(e.position, text.chars().count() - 3); // 外側の `]`
        assert!(e.message.contains("命令"), "{}", e.message);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | grep -E "^test .*(FAILED|failed)|test result"`
Expected: 上で足した 5 つが FAIL(`:` が不明な文字、ネストが「ネストできません」)。

- [ ] **Step 3: 実装する**

(a) 定数を追加する(`pub const MAX_NOTES: usize = 3000;` の次):

```rust
/// 繰り返しを展開した後の命令数の上限。音符・休符は 3000 音・90 秒で抑えられるが、音を出さない命令
/// (`o` `v` など)は抑えられないので、ネストで展開が膨らむ入力のために命令数そのものを制限する。
/// v1 で通る最大の展開(ループ 1 組で本体が約 4996 文字、99 回で約 49.5 万)を弾かない値。
pub const MAX_EXPANDED_OPS: usize = 500_000;
```

(b) `/// 1 トラック分の文字列を命令列にする。` のドキュメントコメントの直前に追加する:

```rust
/// 開いている繰り返し `[`。
struct LoopFrame {
    /// `out` での本体の開始位置
    start: usize,
    /// `[` の元の文字位置
    pos: usize,
    /// `:` が置かれた `out` での位置(最後の回は、ここから後ろを飛ばす)
    colon: Option<usize>,
}

/// 繰り返しの複製を `out` に足す。展開後の命令数が上限を超えるなら、足さずに `pos`(`]` の位置)でエラーにする。
fn extend_checked(out: &mut Vec<Item>, part: &[Item], pos: usize) -> Result<(), MmlError> {
    if out.len() + part.len() > MAX_EXPANDED_OPS {
        return Err(mml_err(
            pos,
            format!("繰り返しを展開した命令が {MAX_EXPANDED_OPS} 個を超えました"),
        ));
    }
    out.extend_from_slice(part);
    Ok(())
}
```

(c) `parse_track` のドキュメントコメントを差し替える。

置き換える前:

```rust
/// 1 トラック分の文字列を命令列にする。繰り返し `[ ... ]n` はここで展開する。
/// 繰り返しは入れ子にできないので、展開後の命令数は入力サイズ(5000 文字 × 最大 99 回)で抑えられる。
/// 時間の上限はここでは見ず、`run_track` が実行しながら判定する。
```

置き換えた後:

```rust
/// 1 トラック分の文字列を命令列にする。繰り返し `[ ... ]n` はここで展開する。
/// 繰り返しは入れ子にできる。展開後の命令数は `MAX_EXPANDED_OPS` で抑える。
/// 時間の上限はここでは見ず、`run_track` が実行しながら判定する。
```

(d) `parse_track` の中の宣言を置き換える。

置き換える前:

```rust
    // 開いている繰り返しの (out の開始 index, `[` の位置)
    let mut open_loop: Option<(usize, usize)> = None;
```

置き換えた後:

```rust
    let mut loops: Vec<LoopFrame> = Vec::new();
```

(e) `'[' => { ... }` と `']' => { ... }` の 2 つのアームを、次の 3 つのアームに置き換える。

置き換える前:

```rust
            '[' => {
                if open_loop.is_some() {
                    return Err(mml_err(pos, "繰り返しはネストできません"));
                }
                open_loop = Some((out.len(), pos));
                continue;
            }
            ']' => {
                let Some((start, _)) = open_loop.take() else {
                    return Err(mml_err(pos, "対応する `[` がありません"));
                };
                let count = match c.number() {
                    Some((n, p)) => {
                        if !(2..=99).contains(&n) {
                            return Err(mml_err(p, "繰り返し回数は 2〜99 です"));
                        }
                        n
                    }
                    None => 2,
                };
                let body: Vec<Item> = out[start..].to_vec();
                for _ in 1..count {
                    out.extend(body.iter().cloned());
                }
                continue;
            }
```

置き換えた後:

```rust
            '[' => {
                loops.push(LoopFrame {
                    start: out.len(),
                    pos,
                    colon: None,
                });
                continue;
            }
            ':' => {
                let Some(frame) = loops.last_mut() else {
                    return Err(mml_err(pos, "`:` は繰り返し `[ ]` の中でだけ使えます"));
                };
                if frame.colon.is_some() {
                    return Err(mml_err(pos, "`:` は繰り返し 1 つにつき 1 個までです"));
                }
                frame.colon = Some(out.len());
                continue;
            }
            ']' => {
                let Some(LoopFrame { start, colon, .. }) = loops.pop() else {
                    return Err(mml_err(pos, "対応する `[` がありません"));
                };
                let count = match c.number() {
                    Some((n, p)) => {
                        if !(2..=99).contains(&n) {
                            return Err(mml_err(p, "繰り返し回数は 2〜99 です"));
                        }
                        n
                    }
                    None => 2,
                };
                let body: Vec<Item> = out[start..].to_vec();
                // 最初の 1 周は `out` に入っている。`:` があれば、最後の 1 周は `:` の手前までだけを足す
                // (`:` が無ければ、全部を n-1 回足す。あれば、全部を n-2 回と、手前までを 1 回)。
                let head = colon.map(|c| c - start);
                let full_rounds = count - 1 - u32::from(head.is_some());
                for _ in 0..full_rounds {
                    extend_checked(&mut out, &body, pos)?;
                }
                if let Some(head) = head {
                    extend_checked(&mut out, &body[..head], pos)?;
                }
                continue;
            }
```

(f) `parse_track` の末尾の閉じ忘れの検査を置き換える。

置き換える前:

```rust
    if let Some((_, p)) = open_loop {
        return Err(mml_err(p, "閉じていない `[` があります"));
    }
    Ok(out)
```

置き換えた後:

```rust
    if let Some(frame) = loops.last() {
        return Err(mml_err(frame.pos, "閉じていない `[` があります"));
    }
    Ok(out)
```

- [ ] **Step 4: テストが通ることを確認し、最大展開の所要時間を確かめる**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`(`the_largest_v1_loop_still_parses` を含む)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::the_largest 2>&1 | tail -5; cargo test --lib mml::tests::nested_loop 2>&1 | tail -5`
Expected: どちらも `finished in` が数秒以内。**5 秒を超えたら、先へ進まず、ユーザーに所要時間を報告する**(上限の値を見直す根拠になる。spec の「未確認・リスク」)。

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs && echo fmt-ok
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML のループに脱出 : とネストを追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 連符 `{ … }n`

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(`Op`、`LoopFrame` から `parse_track` と `run_track` の終わりまでを置き換え、tests)

**Interfaces:**
- Consumes: Task 2〜5 の `Op::TieLen` / `Op::VolumeDelta` / `Op::Gate`、`MAX_EXPANDED_OPS`、`extend_checked`、`gated`、`MAX_GATE`。
- Produces: `Op::TupletStart` / `Op::TupletEnd { len, dots }`、`enum Frame`(`LoopFrame` を置き換える)、`fn beats(len: u32, dots: u8) -> f64`、`fn duration_sec(len: u32, dots: u8, tempo: u32, scale: f64) -> f64`(4 引数に変わる)、`fn tuplet_scale(rest: &[Item], l_at_open: u32) -> f64`、`TrackState.tuplet_scale`。

- [ ] **Step 1: 失敗するテストを書く**

```rust
    #[test]
    fn tuplet_fits_in_the_given_length() {
        let s = must_parse("{c d e}4");
        assert_eq!(s.notes.len(), 3);
        // 四分音符 1 つ分(0.5 秒)を 3 等分
        approx(s.notes[0].start_sec, 0.0);
        approx(s.notes[1].start_sec, 0.5 / 3.0);
        approx(s.notes[2].start_sec, 1.0 / 3.0);
        approx(s.notes[0].duration_sec, 0.5 / 3.0);
        approx(s.total_sec, 0.5);
        // 長さ省略は `{` の時点の l
        approx(must_parse("l8 {c d e}").total_sec, 0.25);
        // 付点
        approx(must_parse("{c d e}4.").total_sec, 0.75);
        // 連符の後は元の長さに戻る
        let s = must_parse("{c d e}4 f4");
        approx(s.notes[3].start_sec, 0.5);
        approx(s.notes[3].duration_sec, 0.5);
        assert_eq!(must_parse("{C D E}4"), must_parse("{c d e}4")); // 大文字・小文字は区別しない
    }

    #[test]
    fn tuplet_divides_by_the_ratio_of_the_inner_lengths() {
        let s = must_parse("{c4 d8}4"); // 1 拍 : 0.5 拍 を、1 拍に収める
        approx(s.notes[0].duration_sec, 0.5 * 2.0 / 3.0);
        approx(s.notes[1].start_sec, 0.5 * 2.0 / 3.0);
        approx(s.total_sec, 0.5);
        // 中の l の変更も追う(`{` の時点の l は 4、中は l8 の 3 音)
        let s = must_parse("{l8 c d e}4");
        approx(s.notes[0].duration_sec, 0.5 / 3.0);
        approx(s.total_sec, 0.5);
        // 中の `^` も長さに数える
        let s = must_parse("{c4^4 d4}2");
        assert_eq!(s.notes.len(), 2);
        approx(s.notes[0].duration_sec, 2.0 / 3.0);
        approx(s.notes[1].start_sec, 2.0 / 3.0);
        approx(s.total_sec, 1.0);
        // 休符も数える
        let s = must_parse("{c r e}4");
        assert_eq!(s.notes.len(), 2);
        approx(s.notes[1].start_sec, 1.0 / 3.0);
        approx(s.total_sec, 0.5);
        // 中でテンポを変えても、縮小率(拍の比)は変わらない
        let s = must_parse("{c t60 d e}4");
        approx(s.total_sec, 0.5 / 3.0 + 2.0 / 3.0);
    }

    #[test]
    fn tuplets_combine_with_loops_and_q() {
        let s = must_parse("[{c d e}4]2");
        assert_eq!(s.notes.len(), 6);
        approx(s.total_sec, 1.0);
        let s = must_parse("{[c]2 e}4");
        assert_eq!(s.notes.len(), 3);
        approx(s.total_sec, 0.5);
        // q は縮めた長さに掛かる
        let s = must_parse("q50 {c d e}4");
        approx(s.notes[0].duration_sec, 0.5 / 3.0 / 2.0);
        approx(s.notes[1].start_sec, 0.5 / 3.0);
    }

    #[test]
    fn invalid_tuplets() {
        assert_eq!(must_fail("{c d e").position, 0); // 閉じていない
        assert_eq!(must_fail("c d e}4").position, 5); // 開いていない
        assert_eq!(must_fail("{}4 c").position, 0); // 空
        assert_eq!(must_fail("{o5}4 c").position, 0); // 音符も休符もない
        assert_eq!(must_fail("{c {d}4}4").position, 3); // 入れ子
        assert_eq!(must_fail("{c;d}4").position, 0); // `;` は連符をまたげない
        assert_eq!(must_fail("{c d}3").position, 5); // 長さが不正
        assert_eq!(must_fail("{[c}2]").position, 1); // 括弧の交差(`[` が閉じていない)
        assert_eq!(must_fail("[{c]2}").position, 1); // 括弧の交差(`{` が閉じていない)
        assert_eq!(must_fail("{c [ {d}4 ]2}4").position, 5); // ループの中でも入れ子は不可
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | grep -E "^test .*(FAILED|failed)|test result"`
Expected: 上で足した 4 つが FAIL(`{` が不明な文字)。

- [ ] **Step 3: 実装する**

(a) `enum Op` の `Gate(u8),` の次に追加する:

```rust
    /// `{`。対応する `}` までの音符・休符・`^` の長さを、`}` の後の長さに収める。
    TupletStart,
    /// `}<長さ>`。長さ省略は `{` の時点の `l` の値。
    TupletEnd {
        len: Option<u32>,
        dots: u8,
    },
```

(b) ファイルの `/// 開いている繰り返し `[`。` で始まる `struct LoopFrame` の定義の行から、`run_track` 関数の閉じ `}` まで(`/// MML テキストを解釈する。` の直前まで)を、次のコード**全体**で置き換える。(途中の `duration_sec` と `check_limit` と `TrackState` も含めて置き換える。`Cursor` と `Op` と `Item` と `mml_err` と定数は触らない。)

```rust
/// 開いている括弧。繰り返し `[` と連符 `{` を、1 つのスタックで管理する(交差を弾くため)。
enum Frame {
    Loop {
        /// `out` での本体の開始位置
        start: usize,
        /// `[` の元の文字位置
        pos: usize,
        /// `:` が置かれた `out` での位置(最後の回は、ここから後ろを飛ばす)
        colon: Option<usize>,
    },
    Tuplet {
        /// `out` での `TupletStart` の位置
        start: usize,
        /// `{` の元の文字位置
        pos: usize,
    },
}

/// 繰り返しの複製を `out` に足す。展開後の命令数が上限を超えるなら、足さずに `pos`(`]` の位置)でエラーにする。
fn extend_checked(out: &mut Vec<Item>, part: &[Item], pos: usize) -> Result<(), MmlError> {
    if out.len() + part.len() > MAX_EXPANDED_OPS {
        return Err(mml_err(
            pos,
            format!("繰り返しを展開した命令が {MAX_EXPANDED_OPS} 個を超えました"),
        ));
    }
    out.extend_from_slice(part);
    Ok(())
}

/// 1 トラック分の文字列を命令列にする。繰り返し `[ ... ]n` はここで展開する。
/// 繰り返しは入れ子にできる。展開後の命令数は `MAX_EXPANDED_OPS` で抑える。
/// 連符 `{ ... }n` は入れ子にできず、繰り返しとは括弧が交差しない限り組み合わせられる。
/// 時間の上限はここでは見ず、`run_track` が実行しながら判定する。
fn parse_track(syms: &[(char, usize)], end: usize) -> Result<Vec<Item>, MmlError> {
    let mut c = Cursor { syms, i: 0, end };
    let mut out: Vec<Item> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();

    while let Some(ch) = c.peek() {
        let pos = c.pos();
        c.bump();
        let op = match ch {
            'c' | 'd' | 'e' | 'f' | 'g' | 'a' | 'b' => {
                let base = match ch {
                    'c' => 0,
                    'd' => 2,
                    'e' => 4,
                    'f' => 5,
                    'g' => 7,
                    'a' => 9,
                    _ => 11,
                };
                let accidental = match c.peek() {
                    Some('+') | Some('#') => {
                        c.bump();
                        1
                    }
                    Some('-') => {
                        c.bump();
                        -1
                    }
                    _ => 0,
                };
                let (len, dots) = c.length_and_dots()?;
                Op::Note {
                    semitone: base + accidental,
                    len,
                    dots,
                }
            }
            'r' => {
                let (len, dots) = c.length_and_dots()?;
                Op::Rest { len, dots }
            }
            '&' => Op::Tie,
            '^' => {
                let (len, dots) = c.length_and_dots()?;
                Op::TieLen { len, dots }
            }
            '>' => Op::OctaveUp,
            '<' => Op::OctaveDown,
            'o' => {
                let (n, p) = c.require_number(pos, 'o')?;
                if !(1..=8).contains(&n) {
                    return Err(mml_err(p, "オクターブは 1〜8 です"));
                }
                Op::Octave(n as u8)
            }
            'l' => {
                let (n, p) = c.require_number(pos, 'l')?;
                if !VALID_LENGTHS.contains(&n) {
                    return Err(mml_err(p, "長さは 1, 2, 4, 8, 16, 32, 64 のいずれかです"));
                }
                Op::DefaultLen(n)
            }
            't' => {
                let (n, p) = c.require_number(pos, 't')?;
                if !(32..=255).contains(&n) {
                    return Err(mml_err(p, "テンポは 32〜255 です"));
                }
                Op::Tempo(n)
            }
            'v' => {
                if let Some(sign @ ('+' | '-')) = c.peek() {
                    c.bump();
                    let n = match c.number() {
                        Some((n, p)) => {
                            if !(1..=15).contains(&n) {
                                return Err(mml_err(p, "相対音量は 1〜15 です"));
                            }
                            n
                        }
                        None => 1,
                    };
                    Op::VolumeDelta(if sign == '+' { n as i32 } else { -(n as i32) })
                } else {
                    let (n, p) = c.require_number(pos, 'v')?;
                    if n > 15 {
                        return Err(mml_err(p, "音量は 0〜15 です"));
                    }
                    Op::Volume(n as u8)
                }
            }
            'q' => {
                let (n, p) = c.require_number(pos, 'q')?;
                if !(1..=MAX_GATE).contains(&n) {
                    return Err(mml_err(p, format!("ゲートタイムは 1〜{MAX_GATE} です")));
                }
                Op::Gate(n as u8)
            }
            '@' => {
                if c.peek().is_none() {
                    return Err(mml_err(pos, "`@` の後に音色の番号か名前が必要です"));
                }
                if let Some((n, p)) = c.number() {
                    let Some(t) = Timbre::from_number(n) else {
                        return Err(mml_err(
                            p,
                            format!("音色は 0〜{} です", Timbre::ALL.len() - 1),
                        ));
                    };
                    Op::Timbre(t)
                } else if let Some(t) = c.timbre_name() {
                    Op::Timbre(t)
                } else {
                    return Err(mml_err(c.pos(), "不明な音色です"));
                }
            }
            '[' => {
                stack.push(Frame::Loop {
                    start: out.len(),
                    pos,
                    colon: None,
                });
                continue;
            }
            ':' => match stack.last_mut() {
                Some(Frame::Loop { colon, .. }) if colon.is_none() => {
                    *colon = Some(out.len());
                    continue;
                }
                Some(Frame::Loop { .. }) => {
                    return Err(mml_err(pos, "`:` は繰り返し 1 つにつき 1 個までです"));
                }
                _ => {
                    return Err(mml_err(pos, "`:` は繰り返し `[ ]` の直下にだけ置けます"));
                }
            },
            ']' => {
                let (start, colon) = match stack.pop() {
                    Some(Frame::Loop { start, colon, .. }) => (start, colon),
                    Some(Frame::Tuplet { pos: open, .. }) => {
                        return Err(mml_err(open, "閉じていない `{` があります"));
                    }
                    None => return Err(mml_err(pos, "対応する `[` がありません")),
                };
                let count = match c.number() {
                    Some((n, p)) => {
                        if !(2..=99).contains(&n) {
                            return Err(mml_err(p, "繰り返し回数は 2〜99 です"));
                        }
                        n
                    }
                    None => 2,
                };
                let body: Vec<Item> = out[start..].to_vec();
                // 最初の 1 周は `out` に入っている。`:` があれば、最後の 1 周は `:` の手前までだけを足す
                // (`:` が無ければ、全部を n-1 回足す。あれば、全部を n-2 回と、手前までを 1 回)。
                let head = colon.map(|c| c - start);
                let full_rounds = count - 1 - u32::from(head.is_some());
                for _ in 0..full_rounds {
                    extend_checked(&mut out, &body, pos)?;
                }
                if let Some(head) = head {
                    extend_checked(&mut out, &body[..head], pos)?;
                }
                continue;
            }
            '{' => {
                if stack.iter().any(|f| matches!(f, Frame::Tuplet { .. })) {
                    return Err(mml_err(pos, "連符はネストできません"));
                }
                stack.push(Frame::Tuplet {
                    start: out.len(),
                    pos,
                });
                Op::TupletStart
            }
            '}' => {
                let (start, open) = match stack.pop() {
                    Some(Frame::Tuplet { start, pos: open }) => (start, open),
                    Some(Frame::Loop { pos: open, .. }) => {
                        return Err(mml_err(open, "閉じていない `[` があります"));
                    }
                    None => return Err(mml_err(pos, "対応する `{` がありません")),
                };
                let (len, dots) = c.length_and_dots()?;
                let has_length = out[start + 1..]
                    .iter()
                    .any(|it| matches!(it.op, Op::Note { .. } | Op::Rest { .. } | Op::TieLen { .. }));
                if !has_length {
                    return Err(mml_err(open, "連符の中に音符か休符が必要です"));
                }
                Op::TupletEnd { len, dots }
            }
            other => return Err(mml_err(pos, format!("不明な文字 `{other}` です"))),
        };
        out.push(Item { op, pos });
    }

    match stack.last() {
        Some(Frame::Loop { pos, .. }) => Err(mml_err(*pos, "閉じていない `[` があります")),
        Some(Frame::Tuplet { pos, .. }) => Err(mml_err(*pos, "閉じていない `{` があります")),
        None => Ok(out),
    }
}

struct TrackState {
    octave: i32,
    len: u32,
    tempo: u32,
    volume: u32,
    gate: u32,
    timbre: Timbre,
    t: f64,
    /// 直前の命令が音符だったとき、その音符の `notes` 内の index(タイの連結先)
    last_note: Option<usize>,
    /// 直前の音符の全長(秒。ゲートを掛ける前。タイで延びる)
    last_full: f64,
    /// 直前の音符を作ったときのゲート(タイで延びても、先頭の音符のものを使う)
    last_gate: u32,
    /// 直前に `&` があったとき、その位置
    tie_pos: Option<usize>,
    /// 連符の中で、拍の長さに掛ける縮小率(連符の外では 1.0)
    tuplet_scale: f64,
}

/// 長さ(拍。四分音符が 1)。
fn beats(len: u32, dots: u8) -> f64 {
    let dot_factor = match dots {
        0 => 1.0,
        1 => 1.5,
        _ => 1.75,
    };
    4.0 / f64::from(len) * dot_factor
}

/// 長さを秒にする。`scale` は連符の縮小率(連符の外では 1.0。1.0 を掛けても値は変わらない)。
fn duration_sec(len: u32, dots: u8, tempo: u32, scale: f64) -> f64 {
    beats(len, dots) * scale * 60.0 / f64::from(tempo)
}

/// 全長にゲートタイム(百分率)を掛ける。100 のときは掛け算をせず、v1 と同じ値のまま返す。
fn gated(full: f64, gate: u32) -> f64 {
    if gate >= MAX_GATE {
        full
    } else {
        full * f64::from(gate) / f64::from(MAX_GATE)
    }
}

fn check_limit(t: f64, pos: usize) -> Result<(), MmlError> {
    if t > MAX_TOTAL_SEC + TIME_EPS {
        return Err(mml_err(
            pos,
            format!("再生時間は {} 秒以下にしてください", MAX_TOTAL_SEC as u32),
        ));
    }
    Ok(())
}

/// `{` の直後の命令列 `rest`(最初の `TupletEnd` まで)から、連符の縮小率を求める。
/// 縮小率 = `}` の後の長さ(拍) ÷ 中の音符・休符・`^` の長さの合計(拍)。中の `l` の変更も追う。
/// `}` の長さが省略されているときは、`{` の時点の `l`(`l_at_open`)を使う。
/// 連符は入れ子にならず、`parse_track` が `{` と `}` を必ず対にするので、`TupletEnd` は必ずある。
fn tuplet_scale(rest: &[Item], l_at_open: u32) -> f64 {
    let mut l = l_at_open;
    let mut sum = 0.0;
    for it in rest {
        match &it.op {
            Op::DefaultLen(n) => l = *n,
            Op::Note { len, dots, .. } | Op::Rest { len, dots } | Op::TieLen { len, dots } => {
                sum += beats(len.unwrap_or(l), *dots);
            }
            Op::TupletEnd { len, dots } => {
                return beats(len.unwrap_or(l_at_open), *dots) / sum;
            }
            _ => {}
        }
    }
    unreachable!("parse_track pairs every `{{` with a `}}`")
}

/// 命令列を実行して音を `notes` に積み、トラックの終了時刻(秒)を返す。
/// 時間の積算は音符・休符ごとに行い、上限を超えた最初の音符・休符でエラーにして打ち切る。
/// (繰り返しは `parse_track` で先に展開済み。その大きさは `MAX_EXPANDED_OPS` で抑えられている。)
fn run_track(track: u8, items: &[Item], notes: &mut Vec<NoteEvent>) -> Result<f64, MmlError> {
    let mut st = TrackState {
        octave: DEFAULT_OCTAVE,
        len: DEFAULT_LEN,
        tempo: DEFAULT_TEMPO,
        volume: DEFAULT_VOLUME,
        gate: MAX_GATE,
        timbre: Timbre::Fm,
        t: 0.0,
        last_note: None,
        last_full: 0.0,
        last_gate: MAX_GATE,
        tie_pos: None,
        tuplet_scale: 1.0,
    };

    for (i, Item { op, pos }) in items.iter().enumerate() {
        let pos = *pos;
        match op {
            Op::Octave(n) => st.octave = i32::from(*n),
            Op::OctaveUp => {
                if st.octave >= 8 {
                    return Err(mml_err(pos, "オクターブは 1〜8 です"));
                }
                st.octave += 1;
            }
            Op::OctaveDown => {
                if st.octave <= 1 {
                    return Err(mml_err(pos, "オクターブは 1〜8 です"));
                }
                st.octave -= 1;
            }
            Op::DefaultLen(n) => st.len = *n,
            Op::Tempo(n) => st.tempo = *n,
            Op::Volume(n) => st.volume = u32::from(*n),
            Op::VolumeDelta(delta) => {
                let v = st.volume as i32 + delta;
                if !(0..=15).contains(&v) {
                    return Err(mml_err(pos, "音量は 0〜15 です"));
                }
                st.volume = v as u32;
            }
            Op::Gate(n) => st.gate = u32::from(*n),
            Op::Timbre(t) => st.timbre = *t,
            Op::TupletStart => st.tuplet_scale = tuplet_scale(&items[i + 1..], st.len),
            Op::TupletEnd { .. } => st.tuplet_scale = 1.0,
            Op::Tie => {
                if st.last_note.is_none() || st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の前に音符が必要です"));
                }
                st.tie_pos = Some(pos);
            }
            Op::TieLen { len, dots } => {
                if st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の直後には `^` を置けません"));
                }
                let Some(idx) = st.last_note else {
                    return Err(mml_err(pos, "`^` の前に音符が必要です"));
                };
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                st.last_full += d;
                notes[idx].duration_sec = gated(st.last_full, st.last_gate);
                st.t += d;
                check_limit(st.t, pos)?;
            }
            Op::Rest { len, dots } => {
                if let Some(tp) = st.tie_pos {
                    return Err(mml_err(tp, "`&` の後に音符が必要です"));
                }
                st.t += duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                st.last_note = None;
                check_limit(st.t, pos)?;
            }
            Op::Note {
                semitone,
                len,
                dots,
            } => {
                let midi = 12 * (st.octave + 1) + semitone;
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                if st.tie_pos.take().is_some() {
                    let idx = st.last_note.expect("a tie is only armed after a note");
                    if i32::from(notes[idx].midi) != midi {
                        return Err(mml_err(pos, "`&` は同じ高さの音符どうしだけをつなげます"));
                    }
                    st.last_full += d;
                    notes[idx].duration_sec = gated(st.last_full, st.last_gate);
                } else {
                    if notes.len() >= MAX_NOTES {
                        return Err(mml_err(pos, format!("音符は {MAX_NOTES} 個までです")));
                    }
                    notes.push(NoteEvent {
                        track,
                        start_sec: st.t,
                        duration_sec: gated(d, st.gate),
                        midi: midi as u8,
                        velocity: f64::from(st.volume) / 15.0,
                        timbre: st.timbre,
                    });
                    st.last_note = Some(notes.len() - 1);
                    st.last_full = d;
                    st.last_gate = st.gate;
                }
                st.t += d;
                check_limit(st.t, pos)?;
            }
        }
    }

    if let Some(tp) = st.tie_pos {
        return Err(mml_err(tp, "`&` の後に音符が必要です"));
    }
    Ok(st.t)
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`(Task 1〜5 のテストと既存のテスト全部を含む)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|generates_frontend_bindings"`
Expected: 失敗なし。`generates_frontend_bindings` も通り、`git -C /home/onodai145/repos/github.com/onodai145/tsumugi status --short` に `tauri.gen.ts` が出ない(型を変えていないので)。

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs && echo fmt-ok
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML に連符 { }n を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: フロント — コメントの範囲、字句解析、使用量の数え方、エディタの色

**Files:**
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlComments.ts`
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlComments.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlLexer.ts`、`mmlLexer.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlUsage.ts`、`mmlUsage.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/ui/settings/mmlEditorExtensions.ts`、`mmlEditorExtensions.test.ts`

**Interfaces:**
- Produces: `findComments(text: string): CommentRange[]`、`blankComments(text: string): string`(`mmlComments.ts`)、`MmlTokenKind` に `"comment"` を追加。`lexMml` は `from` 昇順の `MmlToken[]` を返す(コメントのトークンを含む)。
- Consumes: Rust 側の規則(`mml.rs` の `symbols`: `//` は改行の手前まで、`/* */` は入れ子不可、閉じ忘れは文末まで、`/` 単独はコメントでない)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlComments.test.ts` を新規作成する:

```ts
import { describe, expect, it } from "vitest";
import { blankComments, findComments } from "./mmlComments";

describe("findComments", () => {
  it("行コメントは改行の手前まで(改行は含めない)", () => {
    expect(findComments("c // d\ne")).toEqual([{ from: 2, to: 6 }]);
    expect(findComments("c//x")).toEqual([{ from: 1, to: 4 }]);
    expect(findComments("c // x\r\nd")).toEqual([{ from: 2, to: 7 }]); // \r は行コメントの中
  });

  it("範囲コメントは `*/` まで。改行をまたげる", () => {
    expect(findComments("c/* x */d")).toEqual([{ from: 1, to: 8 }]);
    expect(findComments("c/* x\ny */d")).toEqual([{ from: 1, to: 10 }]);
    expect(findComments("a/**/b")).toEqual([{ from: 1, to: 5 }]);
  });

  it("入れ子にできない: 最初の `*/` で閉じる", () => {
    expect(findComments("/* a /* b */ c */")).toEqual([{ from: 0, to: 12 }]);
  });

  it("閉じていない `/*` は文末まで", () => {
    expect(findComments("c /* d")).toEqual([{ from: 2, to: 6 }]);
    expect(findComments("c/*/")).toEqual([{ from: 1, to: 4 }]); // `/*/` は閉じていない
  });

  it("`/` 単独や、間に空白のある `/ /` はコメントではない", () => {
    expect(findComments("c/d")).toEqual([]);
    expect(findComments("c / / d")).toEqual([]);
  });

  it("コメントの中の `//` `/*` は、外側のコメントに含まれる", () => {
    expect(findComments("// /* \nc /* // */")).toEqual([
      { from: 0, to: 6 },
      { from: 9, to: 17 },
    ]);
  });

  it("位置は UTF-16 の添字(サロゲートペアは 2)", () => {
    expect(findComments("😀//x")).toEqual([{ from: 2, to: 5 }]);
  });

  it("コメントが無ければ空", () => {
    expect(findComments("")).toEqual([]);
    expect(findComments("c4 d4")).toEqual([]);
  });
});

describe("blankComments", () => {
  it("コメントを空白にし、長さと添字を変えない", () => {
    const text = "c // d\ne";
    const out = blankComments(text);
    expect(out).toHaveLength(text.length);
    expect(out.replace(/\s/g, "")).toBe("ce");
    expect(out[0]).toBe("c");
    expect(out[text.length - 1]).toBe("e");
  });

  it("範囲コメントは、改行を含めて空白になる", () => {
    expect(blankComments("c/* x\ny */d")).toBe("c" + " ".repeat(9) + "d");
  });

  it("コメントが無ければ同じ文字列", () => {
    expect(blankComments("c4 d4")).toBe("c4 d4");
  });

  it("コメント内の `;` が残らない", () => {
    expect(blankComments("c // ; d\ne")).not.toContain(";");
  });
});
```

`frontend/src/lib/mmlLexer.test.ts` の末尾の `describe("lexMml", () => { ... });` の**中**(最後の `it` の後)に追加する:

```ts
  it("^ はタイ(rest)で、後ろの長さと付点は number", () => {
    expect(lex("c4^8.")).toEqual([
      ["c", "note"],
      ["4", "number"],
      ["^", "rest"],
      ["8.", "number"],
    ]);
  });

  it("v+n と v-n は 1 つのコマンド(符号まで)と number、q は数字つきコマンド", () => {
    expect(lex("v+2 v- q50")).toEqual([
      ["v+", "command"],
      ["2", "number"],
      ["v-", "command"],
      ["q", "command"],
      ["50", "number"],
    ]);
    expect(lex("v10")).toEqual([
      ["v", "command"],
      ["10", "number"],
    ]);
  });

  it("連符は { と、} とその直後の長さ・付点(repeat)", () => {
    expect(lex("{cde}4.")).toEqual([
      ["{", "repeat"],
      ["c", "note"],
      ["d", "note"],
      ["e", "note"],
      ["}4.", "repeat"],
    ]);
  });

  it("ループの脱出 : は repeat", () => {
    expect(lex("[c:d]2")).toEqual([
      ["[", "repeat"],
      ["c", "note"],
      [":", "repeat"],
      ["d", "note"],
      ["]2", "repeat"],
    ]);
  });

  it("行コメントは改行の手前までが comment で、中の記号は色分けしない", () => {
    expect(lex("c // t120 [\ne")).toEqual([
      ["c", "note"],
      ["// t120 [", "comment"],
      ["e", "note"],
    ]);
  });

  it("範囲コメントは複数行にまたがり、閉じていなければ文末まで", () => {
    expect(lex("c/* o5\n; */d")).toEqual([
      ["c", "note"],
      ["/* o5\n; */", "comment"],
      ["d", "note"],
    ]);
    expect(lex("c /* d ; e")).toEqual([
      ["c", "note"],
      ["/* d ; e", "comment"],
    ]);
  });

  it("/ 単独は unknown(コメントではない)", () => {
    expect(lex("c/d")).toEqual([
      ["c", "note"],
      ["/", "unknown"],
      ["d", "note"],
    ]);
  });

  it("コメントを含んでも、トークンは from 昇順", () => {
    const tokens = lexMml("t120 // x\n[c#4.^8 d-]3 /* y */ {e f}4 q80 v+1 ; g");
    for (let i = 1; i < tokens.length; i++) expect(tokens[i].from).toBeGreaterThanOrEqual(tokens[i - 1].from);
  });
```

`frontend/src/lib/mmlUsage.test.ts` の `describe("countTracks", ...)` の**中**(最後の `it` の後)に追加する:

```ts
  it("コメントの中の ; は数えない(パーサーはコメントを除いてからトラックに分ける)", () => {
    expect(countTracks("c4 // ; d4\ne4")).toBe(1);
    expect(countTracks("c4 /* ; */ e4")).toBe(1);
    expect(countTracks("c4 /* x */ ; e4")).toBe(2);
    expect(countTracks("// 全部コメント ;")).toBe(0);
  });
```

`frontend/src/ui/settings/mmlEditorExtensions.test.ts` の `describe("mmlHighlight", ...)` の**中**(最後の `it` の後)に追加する:

```ts
  it("コメントに色のクラスを付け、中の記号は色分けしない", () => {
    const { texts } = mount("c4 // t120 [\nd4 /* o5 */ e4");
    expect(texts("mml-comment")).toEqual(["// t120 [", "/* o5 */"]);
    expect(texts("mml-command")).toEqual([]);
    expect(texts("mml-repeat")).toEqual([]);
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlComments.test.ts src/lib/mmlLexer.test.ts src/lib/mmlUsage.test.ts src/ui/settings/mmlEditorExtensions.test.ts 2>&1 | tail -30`
Expected: `mmlComments.test.ts` は import 失敗(ファイルが無い)、字句解析・使用量・拡張のテストは新しい `it` が FAIL。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlComments.ts` を新規作成する:

```ts
// MML のコメント(Issue #499)。`// …` は行末(改行の手前)まで、`/* … */` は入れ子にできない範囲。
// 閉じていない `/*` は文末まで。Rust の `mml.rs` の `symbols` と同じ規則(構文の検証は Rust 側の parse_mml)。
// `/` 単独はコメントではない。範囲は元のテキストの添字(UTF-16)。パーサーが数える文字位置
// (Unicode スカラ値)との変換は `mmlPosition.ts` が持つ。

export interface CommentRange {
  from: number;
  to: number;
}

/** コメントの範囲を、昇順で返す。コメントの中の `//` や `/*` は、外側のコメントに含まれる */
export function findComments(text: string): CommentRange[] {
  const out: CommentRange[] = [];
  let i = 0;
  while (i < text.length) {
    if (text[i] === "/" && text[i + 1] === "/") {
      let j = i + 2;
      while (j < text.length && text[j] !== "\n") j++;
      out.push({ from: i, to: j });
      i = j;
    } else if (text[i] === "/" && text[i + 1] === "*") {
      const close = text.indexOf("*/", i + 2);
      const to = close < 0 ? text.length : close + 2;
      out.push({ from: i, to });
      i = to;
    } else {
      i++;
    }
  }
  return out;
}

/** コメントを空白に置き換えた文字列。長さと、コメント以外の各文字の添字は変わらない */
export function blankComments(text: string): string {
  const ranges = findComments(text);
  if (ranges.length === 0) return text;
  let out = "";
  let at = 0;
  for (const { from, to } of ranges) {
    out += text.slice(at, from) + " ".repeat(to - from);
    at = to;
  }
  return out + text.slice(at);
}
```

(b) `frontend/src/lib/mmlLexer.ts` を編集する。

- 先頭の import の次の行に追加する: `import { blankComments, findComments } from "./mmlComments";`
- `MmlTokenKind` の定義を置き換える:

置き換える前:
```ts
export type MmlTokenKind = "note" | "rest" | "command" | "timbre" | "number" | "repeat" | "separator" | "unknown";
```
置き換えた後:
```ts
export type MmlTokenKind =
  | "note"
  | "rest"
  | "command"
  | "timbre"
  | "number"
  | "repeat"
  | "separator"
  | "comment"
  | "unknown";
```

- `NUMBERED_COMMANDS` を置き換える:

置き換える前:
```ts
const NUMBERED_COMMANDS = new Set(["o", "l", "t", "v"]);
```
置き換えた後:
```ts
const NUMBERED_COMMANDS = new Set(["o", "l", "t", "v", "q"]);
```

- `lexMml` の先頭の `const syms = symbols(text);` を置き換える(コメントを空白にしてから読む。添字は変わらない):

置き換える前:
```ts
  const syms = symbols(text);
```
置き換えた後:
```ts
  // コメントは空白にして読み飛ばす(添字は変わらない)。コメントそのものは最後にトークンとして足す
  const syms = symbols(blankComments(text));
```

- `&` の分岐の次に `^` の分岐を足す。

置き換える前:
```ts
    } else if (ch === "&") {
      push(i, i + 1, "rest");
      i++;
    } else if (NUMBERED_COMMANDS.has(ch)) {
      push(i, i + 1, "command");
      const j = digitsEnd(i + 1);
      if (j > i + 1) push(i + 1, j, "number");
      i = j;
    }
```
置き換えた後:
```ts
    } else if (ch === "&") {
      push(i, i + 1, "rest");
      i++;
    } else if (ch === "^") {
      push(i, i + 1, "rest");
      i = lengthAndDots(i + 1);
    } else if (NUMBERED_COMMANDS.has(ch)) {
      // `v+2` `v-2` は、符号までを 1 つのコマンドにする
      const cmdEnd = i + (ch === "v" && (at(i + 1) === "+" || at(i + 1) === "-") ? 2 : 1);
      push(i, cmdEnd, "command");
      const j = digitsEnd(cmdEnd);
      if (j > cmdEnd) push(cmdEnd, j, "number");
      i = j;
    }
```

- `[` / `]` / `;` の分岐のところを置き換える(`{` `}` `:` を足す)。

置き換える前:
```ts
    } else if (ch === "[") {
      push(i, i + 1, "repeat");
      i++;
    } else if (ch === "]") {
      const j = digitsEnd(i + 1);
      push(i, j, "repeat");
      i = j;
    } else if (ch === ";") {
```
置き換えた後:
```ts
    } else if (ch === "[" || ch === "{" || ch === ":") {
      push(i, i + 1, "repeat");
      i++;
    } else if (ch === "]") {
      const j = digitsEnd(i + 1);
      push(i, j, "repeat");
      i = j;
    } else if (ch === "}") {
      // `}` の後の長さと付点まで、1 つのトークンにする
      let j = digitsEnd(i + 1);
      while (at(j) === ".") j++;
      push(i, j, "repeat");
      i = j;
    } else if (ch === ";") {
```

- 関数の最後の `return out;` を置き換える:

置き換える前:
```ts
  return out;
}
```
(`lexMml` の末尾の `return out;` とその閉じ括弧。ファイルの最後にある。)
置き換えた後:
```ts
  for (const { from, to } of findComments(text)) out.push({ from, to, kind: "comment" });
  return out.sort((a, b) => a.from - b.from);
}
```

- ファイル先頭のコメントの 2 行目の後ろに 1 行足す: `// コメント(// と /* */)は mmlComments.ts が範囲を求め、種別 comment のトークンにする。`

(c) `frontend/src/lib/mmlUsage.ts` を編集する。

- import を追加する(既存の import の次の行): `import { blankComments } from "./mmlComments";`
- `countTracks` を置き換える:

置き換える前:
```ts
/** パーサーと同じく、`;` で分けた区間のうち空白以外を含むものの数 */
export function countTracks(text: string): number {
  return text.split(";").filter((seg) => seg.trim() !== "").length;
}
```
置き換えた後:
```ts
/** パーサーと同じく、コメントを除いてから `;` で分けた区間のうち、空白以外を含むものの数 */
export function countTracks(text: string): number {
  return blankComments(text)
    .split(";")
    .filter((seg) => seg.trim() !== "").length;
}
```

(d) `frontend/src/ui/settings/mmlEditorExtensions.ts` を編集する。

- `MARKS` の `separator` の行の次に追加する: `  comment: Decoration.mark({ class: "mml-comment" }),`
- `mmlTheme` の `".mml-separator": ...` の行の次に追加する:

```ts
  ".mml-comment": { color: "var(--text-dim)", fontStyle: "italic" },
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlComments.test.ts src/lib/mmlLexer.test.ts src/lib/mmlUsage.test.ts src/ui/settings/mmlEditorExtensions.test.ts 2>&1 | tail -15`
Expected: すべて PASS。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -8`
Expected: 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件だけは出てよい)。`Record<MmlTokenKind, …>` に `comment` が揃っていること。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlComments.ts frontend/src/lib/mmlComments.test.ts frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts frontend/src/lib/mmlUsage.ts frontend/src/lib/mmlUsage.test.ts frontend/src/ui/settings/mmlEditorExtensions.ts frontend/src/ui/settings/mmlEditorExtensions.test.ts && git commit -m "feat: MML の新しい記法を色分けし、コメントの中の ; をトラックに数えない(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: フロント — 入力補助キー、「記法」ヘルプ、ユーザーガイド

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlKeys.ts`、`mmlKeys.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/ui/settings/SoundChoicePicker.svelte`、`SoundChoicePicker.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/docs/guide/user-guide.md`

**Interfaces:**
- Consumes: Task 7 の `lexMml`(`mmlKeys.test.ts` が、キーだけで打った列に `unknown` が残らないことを確かめる)。
- Produces: 入力補助キーの id `tie-len`(`^`)、`loop-escape`(`:`)、`tuplet-open`(`{`)、`tuplet-close`(`}`)。`<pre>` に `data-testid="mml-syntax-help"`。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlKeys.test.ts` を編集する。

(a) 「修飾は # - . &、オクターブは < >、構造は [ ] ;」の `it` を置き換える:

置き換える前:
```ts
  it("修飾は # - . &、オクターブは < >、構造は [ ] ;", () => {
    const inserts = (id: string) => MML_KEY_GROUPS.find((g) => g.id === id)!.keys.map((k) => k.insert);
    expect(inserts("modifiers")).toEqual(["#", "-", ".", "&"]);
    expect(inserts("octave")).toEqual(["<", ">"]);
    expect(inserts("structure")).toEqual(["[", "]", ";"]);
  });
```
置き換えた後:
```ts
  it("修飾は # - . & ^、オクターブは < >、構造は [ ] : { } ;", () => {
    const inserts = (id: string) => MML_KEY_GROUPS.find((g) => g.id === id)!.keys.map((k) => k.insert);
    expect(inserts("modifiers")).toEqual(["#", "-", ".", "&", "^"]);
    expect(inserts("octave")).toEqual(["<", ">"]);
    expect(inserts("structure")).toEqual(["[", "]", ":", "{", "}", ";"]);
  });
```

(b) 最後の `it`(「キーだけで打った列…」)を置き換える:

置き換える前:
```ts
  it("キーだけで打った列(c# 8. & c 16 > [ d ] 2 ; r 4 <)に、読めない文字が残らない", () => {
    const text = ["c", "sharp", "len-8", "dot", "tie", "c", "len-16", "oct-up", "loop-open", "d", "loop-close", "len-2", "track", "r", "len-4", "oct-down"]
      .map((id) => byId(id).insert)
      .join("");
    expect(lexMml(text).filter((t) => t.kind === "unknown")).toEqual([]);
  });
```
置き換えた後:
```ts
  it("キーだけで打った列(c# 8. & c 16 > [ d : e ] 2 { c d } 4 ^ 8 ; r 4 <)に、読めない文字が残らない", () => {
    const text = [
      "c", "sharp", "len-8", "dot", "tie", "c", "len-16", "oct-up",
      "loop-open", "d", "loop-escape", "e", "loop-close", "len-2",
      "tuplet-open", "c", "d", "tuplet-close", "len-4", "tie-len", "len-8",
      "track", "r", "len-4", "oct-down",
    ]
      .map((id) => byId(id).insert)
      .join("");
    expect(lexMml(text).filter((t) => t.kind === "unknown")).toEqual([]);
  });
```

`frontend/src/ui/settings/SoundChoicePicker.test.ts` の「記法ヘルプに音色の一覧(番号と名前)が出る」の `it` の次に追加する:

```ts
  it("記法ヘルプに、タイ ^・相対音量・q・連符・ループの脱出とネスト・コメントの書き方が出る", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    for (const s of ["c4^8", "v+2", "q80", "{cde}4", "[c:d]3", "//", "/* … */"]) expect(text).toContain(s);
    expect(text).not.toContain("2〜99回、入れ子は不可"); // 繰り返しは入れ子にできるようになった
    expect(text).toContain("全体で 5000 文字");
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts src/ui/settings/SoundChoicePicker.keys.test.ts 2>&1 | tail -25`
Expected: `mmlKeys.test.ts` の 2 つと、`SoundChoicePicker.test.ts` の新しい `it`(`mml-syntax-help` が無い)が FAIL。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlKeys.ts`:

- `modifiers` グループの `tie` キーの次に追加する:

```ts
      { id: "tie-len", label: "^", insert: "^", aria: "長さを足すタイ ^" },
```

- `structure` グループを置き換える(ラベルも変える):

置き換える前:
```ts
    id: "structure",
    label: "繰り返し・トラック",
    keys: [
      { id: "loop-open", label: "[", insert: "[", aria: "繰り返しの始め [" },
      { id: "loop-close", label: "]", insert: "]", aria: "繰り返しの終わり ]" },
      { id: "track", label: ";", insert: ";", aria: "トラック区切り ;" },
    ],
```
置き換えた後:
```ts
    id: "structure",
    label: "繰り返し・連符・トラック",
    keys: [
      { id: "loop-open", label: "[", insert: "[", aria: "繰り返しの始め [" },
      { id: "loop-close", label: "]", insert: "]", aria: "繰り返しの終わり ]" },
      { id: "loop-escape", label: ":", insert: ":", aria: "繰り返しの脱出 :" },
      { id: "tuplet-open", label: "{", insert: "{", aria: "連符の始め {" },
      { id: "tuplet-close", label: "}", insert: "}", aria: "連符の終わり }" },
      { id: "track", label: ";", insert: ";", aria: "トラック区切り ;" },
    ],
```

(b) `frontend/src/ui/settings/SoundChoicePicker.svelte` の「記法」の `<pre>` を置き換える。波括弧は Svelte の式の開始になるので、`&#123;` `&#125;` で書く(`&gt;` `&lt;` と同じ扱い)。

置き換える前(`<pre class="m-0 mt-1 whitespace-pre-wrap font-mono">` から `</pre>` まで):
```svelte
    <pre class="m-0 mt-1 whitespace-pre-wrap font-mono">音符 c d e f g a b（# や + で半音上げ、- で半音下げ）
長さ 音符の後ろに 1 2 4 8 16 32 64、付点は .（例: c8. ）
休符 r4  タイ c4&c8
o4 オクターブ指定  &gt; で上げる  &lt; で下げる
l8 既定の長さ  t120 テンポ  v10 音量(0〜15)
@3 または @bell 音色（下の一覧。何も書かなければ @0 fm）
[cde]3 繰り返し(2〜99回、入れ子は不可)
; でトラックを分ける(最大4つ。テンポ・音色はトラックごとに書く)
全体で 5000 文字・90 秒・3000 音まで</pre>
```
置き換えた後:
```svelte
    <pre class="m-0 mt-1 whitespace-pre-wrap font-mono" data-testid="mml-syntax-help">音符 c d e f g a b（# や + で半音上げ、- で半音下げ）
長さ 音符の後ろに 1 2 4 8 16 32 64、付点は .（例: c8. ）
休符 r4  タイ c4&c8（同じ高さの音をつなぐ）  c4^8（直前の音に長さを足す）
o4 オクターブ指定  &gt; で上げる  &lt; で下げる
l8 既定の長さ  t120 テンポ  v10 音量(0〜15)  v+2 v-2 音量を相対で変える(数字を省くと 1)
q80 音を鳴らす長さの割合(1〜100、既定 100。短くするとスタッカート)
&#123;cde&#125;4 連符(中の長さを合計して、後ろの長さ 1 つ分に収める。入れ子は不可)
@3 または @bell 音色（下の一覧。何も書かなければ @0 fm）
[cde]3 繰り返し(2〜99回、入れ子可)  [c:d]3 最後の回だけ : の後ろを飛ばす
; でトラックを分ける(最大4つ。テンポ・音色はトラックごとに書く)
// 行末までコメント  /* … */ 範囲コメント(; を含んでもトラックは分かれない。文字数には含む)
全体で 5000 文字・90 秒・3000 音まで</pre>
```

(c) `docs/guide/user-guide.md`:

まず、他に「入れ子は不可」を前提にした記述が無いかを確かめる:
Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi && grep -rn "入れ子は不可\|入れ子にでき" docs/guide docs/design README.md e2e 2>/dev/null | grep -v node_modules | head`
Expected: `docs/guide/user-guide.md` の記法表の `[cde]3` の行だけ(他にあれば、同じ趣旨で直す)。

次に、`user-guide.md` を次のとおり編集する(Python などで、置き換え前の文字列がちょうど 1 回出ることを確かめながら置き換える):

1. 記法表の `&` の行の次に、次の 5 行を足す:

```markdown
| `^` | タイ(長さを足す)。`c4^8` のように、直前の音符を後ろの長さだけ伸ばす（長さを省くと `l` の値）。`&` と違い、次の音符を書かずに伸ばせる。休符の後ろには使えない |
| `v+2`、`v-2` | 音量を相対で上げ下げ（1〜15、数字を省くと 1）。結果が 0〜15 を出るとエラー |
| `q80` | 音を鳴らす長さの割合（1〜100、既定 100）。短くするとスタッカートになる。次の音の始まりは変わらない |
| `{cde}4` | 連符。中の音符・休符の長さを合計して、後ろの長さ（省くと `{` の時点の `l`）1 つ分に収める。`{c4 d8}4` のように長さを書いたときは、その比で割る。入れ子は不可 |
| `// …`、`/* … */` | コメント。`//` は行末まで、`/* */` は範囲（入れ子不可、閉じ忘れはエラー）。`;` を含んでもトラックは分かれない。文字数（5000）には含む |
```

2. 記法表の繰り返しの行を置き換える:

置き換える前:
```markdown
| `[cde]3` | 繰り返し（2〜99 回、入れ子は不可） |
```
置き換えた後:
```markdown
| `[cde]3` | 繰り返し（2〜99 回、入れ子可）。`[c:d]3` のように `:` を入れると、最後の回だけ `:` の後ろを飛ばす（`c d c d c`）。展開後の命令が 50 万個を超えるとエラー |
```

3. 説明の段落(「大文字小文字は区別せず、空白と改行は無視します。」で始まる段落)の、その 1 文の直後に追記する。

置き換える前:
```
大文字小文字は区別せず、空白と改行は無視します。全体で 5000 文字
```
置き換えた後:
```
大文字小文字は区別せず、空白と改行は無視します(コメントも読み飛ばします)。全体で 5000 文字
```

4. 「MML で通知音を作る」の段落(`音符・休符・コマンド・音色・数字は色分けされます`)を置き換える:

置き換える前:
```
音符・休符・コマンド・音色・数字は色分けされます。
```
置き換えた後:
```
音符・休符・コマンド・音色・数字・コメントは色分けされます。
```

5. 同じ段落の入力補助キーの列挙を置き換える:

置き換える前:
```
`#` `-` `.` `&`、オクターブ `<` `>`、長さ(`1 2 4 8 16 32`)、`[` `]` `;` を押すと
```
置き換えた後:
```
`#` `-` `.` `&` `^`、オクターブ `<` `>`、長さ(`1 2 4 8 16 32`)、`[` `]` `:` `{` `}` `;` を押すと
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts src/ui/settings/SoundChoicePicker.keys.test.ts 2>&1 | tail -15`
Expected: すべて PASS(`SoundChoicePicker.keys.test.ts` は新しい 4 つのキーも自動で押して確かめる)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -6`
Expected: 0 errors。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlKeys.ts frontend/src/lib/mmlKeys.test.ts frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md && git commit -m "feat: MML の入力補助キー・記法ヘルプ・ガイドを新しい記法に合わせる(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: 全体の確認と実機の確認

**Files:**
- 変更なし(確認だけ。直す必要が出たら、該当タスクの続きとして直す)。確認の記録を spec の末尾に追記する場合のみ、`docs/superpowers/specs/2026-10-20-issue-499-mml-syntax-extension-design.md` を編集する。

- [ ] **Step 1: 自動テストをすべて走らせる**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|panicked" | head
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -6
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm test 2>&1 | tail -8
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short
```
Expected: `cargo test` は失敗 0(`#[ignore]` は除く)。`pnpm check` は 0 errors。`pnpm test` は全件 PASS。`git status` が空(`frontend/src/bindings/tauri.gen.ts` が変わっていない = 型を変えていない)。

- [ ] **Step 2: 実アプリで確認する(隔離した Xvfb のインスタンス)**

実画面に出さず、ユーザーの本物のデータ(`~/.cache/com.onodai.tsumugi/cache.db` など)に触れない。手順は、メモリの次の 3 件を**先に読んでから**従う:
- `feedback-dev-server-verification-must-use-virtual-display.md`(Xvfb 越し。`WAYLAND_DISPLAY` も unset)
- `tauri-single-instance-blocks-xdg-isolated-verification.md`(`dbus-run-session` が要る)
- `reference-isolated-instance-needs-short-xdg-path.md`(隔離インスタンスの bridge のパスは `/tmp` 直下の短いパス)

併せて、`CLAUDE.md` の「debug_bridge」の節(`curl --unix-socket … --data-binary '<js>'`、`return` が要る、JSON を 2 回デコードする)と、メモリ `debug-bridge-does-not-support-await.md`(`await` は使えない。kick-and-poll で待つ)に従う。隔離起動のデバッグビルドは `frontend/dist` を埋め込むので、`cd frontend && pnpm build` のあとに `cd src-tauri && cargo build` する。

確認すること(設定→通知→「MML（打ち込み）」。それぞれ、画面のスクリーンショットを自分で見る):
1. 次の MML を入れて、構文エラーが出ず、再生時間が出て、色分けされる(コメントは薄い斜体、`^` `q80` `v+2` `{` `}` `:` が色分けされる):

```
t120 o5 l8 @epiano q80 v+2 // メロディ
[c d e :f]3 c4^8 {e f g}4
/* 2 本目 */ ; t120 o3 @bass [c g]4
```
2. コメントの中の `;` でトラック数が増えない(使用量の「トラック」が 2/4 のまま)。
3. 閉じていない `/*` を入れると、エラー「`/*` が閉じていません」が出て、「N 文字目」を押すと `/*` にカーソルが移る。
4. `c4&^8` / `{c {d}4}4` / `q0 c` / `v+20 c` で、それぞれ位置つきのエラーが出て、保存と試聴が `disabled` になる。
5. 「記法」を開くと、新しい記法の行が出る(`{cde}4` の波括弧が文字として出ている)。
6. 手順 1 の MML で「試聴」を押し、`play_notify_sound` に `data:audio/wav;base64,…` が渡る(bridge で `fetch` の `ipc://` を包んで記録する。メモリ `reference-verify-cache-metrics-via-debug-bridge.md` や spec #75 の「実装後の確認」の方法)。**音が鳴ること自体は、Xvfb に音声デバイスが無いので確認できない**。そう書く。
7. 保存して再起動し、入力欄に同じ MML が復元される(コメントも残る)。

確認が終わったら、自分で起動した Xvfb / `dbus-run-session` / アプリ / bridge のプロセスを、PID を指定して止める(`pkill` / `killall` は使わない。`ps aux` で自分が起動したものだけを PID で選ぶ。E2E で起動したものは `grep -az TSUMUGI_E2E_RUN_TOKEN /proc/<pid>/environ` で見分ける)。

- [ ] **Step 3: 結果を spec の末尾に記録する**

`docs/superpowers/specs/2026-10-20-issue-499-mml-syntax-extension-design.md` の末尾に、次の節を追記する(実際に確認したことと、確認できなかったことを、事実のとおりに書く。数値と所要時間はこの手順で得た実測値を入れる):

```markdown
## 実装後の確認

(確認した日付・ブランチ・HEAD のコミットを書く。)

確認できたこと:
- 自動テスト: `cargo test` の結果、`pnpm check` の結果、`pnpm test` の結果(件数)。
- 展開後 50 万命令の上限: v1 で通る最大のループが通ること、4 重のネストが「命令が多すぎる」エラーで止まること、それぞれの所要時間。
- 実アプリ(隔離インスタンス): Step 2 の 1〜7 のそれぞれで、何を見て、何が起きたか。

確認できなかったこと:
- 実際に音が鳴ること、聞こえ方(Xvfb に音声デバイスが無い)。
- Android の実機・エミュレーター、macOS / Windows。
```

(上の括弧書きは、実際の値で置き換えること。括弧書きのまま残さない。)

- [ ] **Step 4: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add docs/superpowers/specs/2026-10-20-issue-499-mml-syntax-extension-design.md && git commit -m "docs: MML 構文拡張の実装後の確認を記録(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: ブランチの扱いを決める(ユーザーに確認する)**

`superpowers:finishing-a-development-branch` に従う。PR を作る場合は、`.github/pull_request_template.md` の構成に手で合わせ、本文に `Refs #499` と書く(`Fixes` / `Closes` は書かない。#499 の「中くらい」の候補が未着手で残るため)。`gh pr merge` は `--merge`(squash にしない)。push 後に CI を待つループは作らない(ユーザーが見る)。#499 のチェックボックス(`^`、相対音量、コメント、連符、ループの脱出とネスト、`q`)を更新するかは、ユーザーに確認してから行う。
