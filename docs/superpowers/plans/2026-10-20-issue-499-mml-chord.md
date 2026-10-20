# MML の和音 `'ceg'` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に和音 `'ceg'4.`(1 つのトラックで複数の音を同時に鳴らす)を足し、字句解析・入力補助キー・「記法」ヘルプ・ガイドを追随させる。

**Architecture:** `src-tauri/src/mml.rs` だけを変える(`NoteEvent` / `MmlScore` / `MmlError` / レンダラーは変えない)。`parse_track` が `'…'` を `ChordStart` / `ChordNote` / `ChordEnd` の命令列にし(連符の `TupletStart` / `TupletEnd` と同じ作り)、`run_track` が `ChordEnd` で、音数で割った音量の `NoteEvent` を同じ開始時刻で出す。「直前の音符」の状態を和音の全部の音を指すように広げ、`^` が全部を延ばし、`&` は和音とは組み合わせない。フロントは字句解析・入力補助キー・ヘルプ・ガイドを足す。

**Tech Stack:** Rust(`src-tauri`、`cargo test`)、Svelte 5 + TypeScript(`frontend`、Vitest + jsdom)。

**Spec:** `docs/superpowers/specs/2026-10-20-issue-499-mml-chord-design.md`(前提: `2026-10-20-issue-499-mml-syntax-extension-design.md` / `2026-10-20-issue-499-mml-shared-tempo-design.md`)

## Global Constraints

- 作業ブランチは `feat/issue-499-mml-chord`(作成済み)。`main` には触らない。リポジトリルートは `/home/onodai145/repos/github.com/onodai145/tsumugi`。コマンドは絶対パスで書く(作業ディレクトリが変わっても動くように)。
- v1 と PR #511 / #513 の記法・既定値・範囲は変えない。今まで通っていた MML は、同じ `MmlScore` になる(`'` は今は構文エラーなので、保存済みの MML の意味は変わらない)。
- `NoteEvent` / `MmlScore` / `MmlError` / `Timbre` と `mmlRender.ts` は変えない。`frontend/src/bindings/tauri.gen.ts` は変わらない(変わったら何かがおかしい)。
- 上限は 5000 文字(コメントも含む)・3000 音(和音の各音を 1 音として数える)・4 トラック・90 秒・展開後 `MAX_EXPANDED_OPS = 500_000` 命令(トラックごと)。値は変えない。
- 和音の仕様(spec が正):
  - `'ceg'` + 閉じの `'` の後ろに長さと付点(省略時は `l`)。中に書けるのは、音符(`#` `+` `-` つき)と `<` `>` だけ。大文字・空白は今までどおり。
  - 中の `<` `>` は和音の間だけ効き、和音の後ろは元のオクターブに戻る。範囲(1〜8)の検査は今までと同じ(その `<` `>` の位置でエラー)。
  - 各音の `velocity` は `v` の音量 ÷ 15 ÷ 音数。`q` は全部の音に効く。時間は和音の長さ(`q` を掛ける前)だけ進む。
  - `^` は直前の和音の全部の音を延ばす。`&` は和音の前でも後ろでもエラー(位置は `&`)。
  - エラー位置は元のテキストの文字位置(Unicode スカラ値の添字、0 始まり)。UI は 1 始まりの「N 文字目」。`MmlError.position` は `u32`(テストで `usize` と比べるときは `as usize`)。
- コミットメッセージは件名のみ(本文・箇条書きなし)。ハーネスの指示どおり、末尾に `Co-Authored-By` トレーラーを付ける(下の例は `-m` を 2 回に分けている)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら(GPG の署名がタイムアウトすることがある)、止めてユーザーに報告する(再試行しない)。
- Rust のテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::<名前>`。単一のフロントテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run <パス>`(`pnpm test -- <パス>` はパスを無視して全件走る)。
- `rustfmt --check --edition 2021 src/mml.rs` には、`main` にも既にある 5 箇所の差分がある(サンプルと音色のテスト)。確認は「差分の数が増えていない(5 のまま)」で行い、既存の部分は整形しない(`cargo fmt` を全体に掛けない)。増えたら、追加した部分だけを rustfmt の形に直す。
- **`/tmp` はメモリ上の tmpfs(16GB)。** 大きなもの(cargo のビルド出力、worktree)を `/tmp` に置かない。最終レビューに検証用のビルドをさせるときは、worktree もビルド出力もディスク上(`/home/onodai145/.cache/review-…/`)に置かせ、`CARGO_BUILD_JOBS=4` と `CARGO_PROFILE_DEV_DEBUG=0` を付け、終わったら正確なパスで片付けさせる(前回、`/tmp` に約 7.4GB のビルド出力が載って OOM になった)。
- PR を作るときは、本文に `Fixes #499` / `Closes #499` を書かない(#499 には「中くらい」の候補が未着手で残る)。`Refs #499` と書く。Issue のチェックボックスを更新するのは、ユーザーの確認を取ってから。
- ユーザーに見せる文章(最終報告・質問)は日本語。

## Review Focus

仕様が黙っているが、使う人が踏みやすい入力と条件。それぞれ、持ち主のタスクにテストを足してある。

1. 音数の多い和音と展開: 3000 音ちょうどの和音と、3001 音(超えた音の位置でエラー)。ループで和音を繰り返して 3000 を超える入力(`[…]99`)で、エラーになり、短時間で終わること。(Task 1)
2. 和音の中のオクターブ: `'c>c'`、`'c<c'`、範囲外(`o8 'c>c'`)。和音の後ろが元のオクターブに戻ること。エラー時の位置が `<` `>` であること。(Task 1)
3. `&` と `^` との組み合わせ: `c&'ceg'`、`'ceg'&c`、1 音の和音 `'c'&c`、和音の後ろの `^`、休符の後の `^`。(Task 1)
4. 連符・ループ・`q`・`t=` との組み合わせ: 連符の中の和音が 1 つの長さとして数えられること、和音だけの連符、ループの中の和音、`t=` を和音の後ろに書いたときのエラー。(Task 1)
5. エラー位置: 閉じていない `'`、空の和音、中の他の文字・数字、`;`、コメントや空白を挟んだ和音(`'c/*x*/e'`、`' c e g '`)。位置が元のテキストのままであること。(Task 1)
6. 既存の MML が同じ結果になること: 音符のパースを `read_pitch` に切り出し、`last_note` を `last_notes` に置き換えるので、既存のテストが全部そのまま通ること。(Task 1)
7. フロント: 閉じていない `'` が以降を和音の中として色分けすること、`'` の開きと閉じの判定、キーだけで打った列に読めない文字が残らないこと。(Task 2)

---

### Task 1: Rust — 和音を読み、同時に鳴らす

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(`Op`、`read_pitch` と `has_length_op` の追加、`parse_track` の音符の分岐・`'` の分岐・`}` の検査・末尾の `seen_length`、`tuplet_scale`、`TrackState`、`run_track`、tests)

**Interfaces:**
- Consumes: `Cursor::peek` / `bump` / `pos` / `length_and_dots() -> Result<(Option<u32>, u8), MmlError>`、`mml_err(pos, msg)`、`gated(full, gate)`、`duration_sec(len, dots, tempo, scale)`、`check_limit(t, pos)`、`MAX_NOTES`、`seen_length`(PR #513)、`TrackState`、`Op::OctaveUp` / `OctaveDown`。
- Produces: `Op::ChordStart` / `Op::ChordNote { semitone }` / `Op::ChordEnd { len, dots }`、`fn read_pitch(ch: char, c: &mut Cursor) -> i32`、`fn has_length_op(op: &Op) -> bool`、`TrackState.last_notes: Vec<usize>` / `last_chord: bool` / `chord_octave: i32` / `chord_notes: Vec<(i32, usize)>`(`last_note` は無くなる)。

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾(最後の `}` の直前)に追加する:

```rust
    #[test]
    fn chord_plays_all_notes_together() {
        let s = must_parse("'ceg'");
        assert_eq!(s.notes.len(), 3);
        let midis: Vec<u8> = s.notes.iter().map(|n| n.midi).collect();
        assert_eq!(midis, vec![60, 64, 67]); // 書いた順
        for n in &s.notes {
            approx(n.start_sec, 0.0);
            approx(n.duration_sec, 0.5);
        }
        approx(s.total_sec, 0.5);
        // 長さと付点は、閉じの `'` の後ろ。省略は `l`
        approx(must_parse("'ceg'4.").notes[0].duration_sec, 0.75);
        approx(must_parse("l8 'ce'").notes[0].duration_sec, 0.25);
        // 後ろの音符は、和音の長さだけ後
        let s = must_parse("'ceg'4 d4");
        approx(s.notes[3].start_sec, 0.5);
        // 大文字・空白
        assert_eq!(must_parse("' C E G '4"), must_parse("'ceg'4"));
        // 1 音だけの和音も、同じ音を重ねた和音も書ける
        assert_eq!(must_parse("'c'").notes.len(), 1);
        assert_eq!(must_parse("'cc'").notes.len(), 2);
    }

    #[test]
    fn chord_velocity_is_divided_by_the_number_of_notes() {
        approx(must_parse("v15 'ce'").notes[0].velocity, 0.5);
        approx(must_parse("v15 'ceg'").notes[2].velocity, 1.0 / 3.0);
        approx(must_parse("v15 'c'").notes[0].velocity, 1.0); // 1 音は割らない
        approx(must_parse("v15 c").notes[0].velocity, 1.0);
    }

    #[test]
    fn chord_accidentals_and_octaves() {
        let midis = |t: &str| -> Vec<u8> { must_parse(t).notes.iter().map(|n| n.midi).collect() };
        assert_eq!(midis("'c#e-g+'"), vec![61, 63, 68]);
        assert_eq!(midis("o4 'c>c'"), vec![60, 72]);
        assert_eq!(midis("o4 'c<c'"), vec![60, 48]);
        // 和音の後ろは、元のオクターブに戻る
        assert_eq!(midis("o4 'c>c' d"), vec![60, 72, 62]);
        assert_eq!(midis("o4 'c>>c' d"), vec![60, 84, 62]);
        // 和音の外の `>` は、今までどおり残る
        assert_eq!(midis("'c' > d"), vec![60, 74]);
    }

    #[test]
    fn caret_extends_every_note_of_a_chord() {
        let s = must_parse("'ceg'4^4");
        assert_eq!(s.notes.len(), 3);
        for n in &s.notes {
            approx(n.duration_sec, 1.0);
        }
        approx(s.total_sec, 1.0);
        // q は全部の音に効き、延ばした全長に掛かる
        let s = must_parse("q50 'ce'4^4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 0.5);
        let s = must_parse("q50 'ce'4 d4");
        approx(s.notes[0].duration_sec, 0.25);
        approx(s.notes[2].start_sec, 0.5); // 時間は全長だけ進む
    }

    #[test]
    fn chord_combines_with_tuplets_loops_and_shared_tempo() {
        // 連符の中の和音は、1 つの長さとして数える
        let s = must_parse("{'ce' d e}4");
        assert_eq!(s.notes.len(), 4);
        approx(s.notes[0].duration_sec, 0.5 / 3.0);
        approx(s.notes[2].start_sec, 0.5 / 3.0);
        approx(s.total_sec, 0.5);
        // 和音だけの連符
        approx(must_parse("{'ce'}4").total_sec, 0.5);
        // ループ
        let s = must_parse("['ce']2");
        assert_eq!(s.notes.len(), 4);
        approx(s.notes[2].start_sec, 0.5);
        // 共有テンポ
        let s = must_parse("t=60 'ce'4 ; 'g'4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[2].duration_sec, 1.0);
    }

    #[test]
    fn chord_counts_each_note_toward_the_limit() {
        let ok = format!("'{}'", "c".repeat(MAX_NOTES));
        assert_eq!(must_parse(&ok).notes.len(), MAX_NOTES);
        let over = format!("'{}'", "c".repeat(MAX_NOTES + 1));
        assert_eq!(must_fail(&over).position as usize, MAX_NOTES + 1); // 3001 個目の音
        // ループで超える
        let looped = format!("['{}']99", "c".repeat(100));
        let started = std::time::Instant::now();
        let e = must_fail(&looped);
        assert!(e.message.contains("3000"), "{}", e.message);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn invalid_chords() {
        assert_eq!(must_fail("'ce").position, 0); // 閉じていない
        assert_eq!(must_fail("''").position, 0); // 空
        assert_eq!(must_fail("'cr'").position, 2); // 中の休符
        assert_eq!(must_fail("'c[e]2'").position, 2); // 中の `[`
        assert_eq!(must_fail("c 'c4e'").position, 4); // 中の数字
        assert_eq!(must_fail("'ceg'3").position, 5); // 長さが不正(今までと同じ)
        assert_eq!(must_fail("c&'ceg'").position, 1); // `&` の後の和音
        assert_eq!(must_fail("'ceg'&c").position, 5); // 和音の後の `&`
        assert_eq!(must_fail("'c'&c").position, 3); // 1 音の和音でも同じ
        assert_eq!(must_fail("o8 'c>c'").position, 5); // オクターブの範囲外(`>` の位置)
        assert_eq!(must_fail("o1 '<c'").position, 4); // 同じく `<`
        assert_eq!(must_fail("'ce;g'").position, 0); // `;` でトラックが分かれる
        assert_eq!(must_fail("'ce' t=60").position, 5); // `t=` は最初の音符・和音より前
    }

    #[test]
    fn chord_positions_stay_in_the_original_text() {
        // コメントと空白を挟んでも、位置は元のテキストのまま
        assert_eq!(must_fail("/* あ */ 'c x e'").position, 11); // `x` は 12 文字目
        assert_eq!(must_parse("'c/*x*/e'"), must_parse("'ce'"));
        assert_eq!(must_parse("' c e g '"), must_parse("'ceg'"));
    }

    #[test]
    fn a_rest_after_a_chord_forgets_it() {
        // 休符の後の `^` は、今までどおりエラー
        assert_eq!(must_fail("'ce' r ^4").position, 7);
        // 和音の後に普通の音符が来ても、`^` はその音符だけを延ばす
        let s = must_parse("'ce' d ^4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[2].duration_sec, 1.0);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests:: 2>&1 | grep -E "^test .*FAILED|test result"`
Expected: 上で足した 9 つが FAIL(`'` は今は「不明な文字」)。ただし `a_rest_after_a_chord_forgets_it` は、`'` で先に落ちて FAIL する。既存のテストは PASS のまま。

- [ ] **Step 3: 実装する**

(a) `enum Op` の `TupletEnd { ... },` の次(`Timbre(Timbre),` の前)に追加する:

```rust
    /// `'`(和音の開き)。
    ChordStart,
    /// 和音の中の音符。長さを持たず、`ChordEnd` でまとめて鳴らす。
    ChordNote {
        semitone: i32,
    },
    /// `'<長さ>`(和音の閉じ)。長さ省略は `l` の値。
    ChordEnd {
        len: Option<u32>,
        dots: u8,
    },
```

(b) `parse_track` のドキュメントコメント(`/// 1 トラック分の文字列を命令列にする。` で始まる行)の**直前**に、2 つの関数を追加する:

```rust
/// 音名 `ch`(`c`〜`b`)と、続く半音記号(`#` `+` `-`)を読んで、オクターブ内の半音数を返す。
fn read_pitch(ch: char, c: &mut Cursor) -> i32 {
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
    base + accidental
}

/// 時間を進める命令(音符・休符・`^`・和音の閉じ)か。`t=` の置き場所と、連符の中身の検査に使う。
fn has_length_op(op: &Op) -> bool {
    matches!(op, Op::Note { .. } | Op::Rest { .. } | Op::TieLen { .. } | Op::ChordEnd { .. })
}
```

(c) `parse_track` の音符の分岐を置き換える。

置き換える前:
```rust
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
```
置き換えた後:
```rust
            'c' | 'd' | 'e' | 'f' | 'g' | 'a' | 'b' => {
                let semitone = read_pitch(ch, &mut c);
                let (len, dots) = c.length_and_dots()?;
                Op::Note {
                    semitone,
                    len,
                    dots,
                }
            }
```

(d) `parse_track` の `'[' => {` の分岐の**直前**に、和音の分岐を追加する:

```rust
            '\'' => {
                out.push(Item {
                    op: Op::ChordStart,
                    pos,
                });
                let mut count = 0usize;
                loop {
                    let Some(inner) = c.peek() else {
                        return Err(mml_err(pos, "閉じていない `'` があります"));
                    };
                    let inner_pos = c.pos();
                    c.bump();
                    match inner {
                        '\'' => break,
                        'c' | 'd' | 'e' | 'f' | 'g' | 'a' | 'b' => {
                            let semitone = read_pitch(inner, &mut c);
                            out.push(Item {
                                op: Op::ChordNote { semitone },
                                pos: inner_pos,
                            });
                            count += 1;
                        }
                        '<' => out.push(Item {
                            op: Op::OctaveDown,
                            pos: inner_pos,
                        }),
                        '>' => out.push(Item {
                            op: Op::OctaveUp,
                            pos: inner_pos,
                        }),
                        d if d.is_ascii_digit() => {
                            return Err(mml_err(
                                inner_pos,
                                "和音の長さは、閉じの `'` の後ろに書いてください",
                            ));
                        }
                        _ => {
                            return Err(mml_err(
                                inner_pos,
                                "和音の中には音符と `<` `>` だけ書けます",
                            ));
                        }
                    }
                }
                if count == 0 {
                    return Err(mml_err(pos, "和音の中に音符が必要です"));
                }
                let (len, dots) = c.length_and_dots()?;
                Op::ChordEnd { len, dots }
            }
```

(e) `parse_track` の連符の `}` の検査と、末尾の `seen_length` を、`has_length_op` を使うように置き換える。

置き換える前:
```rust
                let has_length = out[start + 1..].iter().any(|it| {
                    matches!(it.op, Op::Note { .. } | Op::Rest { .. } | Op::TieLen { .. })
                });
```
置き換えた後:
```rust
                let has_length = out[start + 1..].iter().any(|it| has_length_op(&it.op));
```

置き換える前:
```rust
        if matches!(op, Op::Note { .. } | Op::Rest { .. } | Op::TieLen { .. }) {
            seen_length = true;
        }
```
置き換えた後:
```rust
        if has_length_op(&op) {
            seen_length = true;
        }
```

(f) `tuplet_scale` の長さを数える分岐を置き換える。

置き換える前:
```rust
            Op::Note { len, dots, .. } | Op::Rest { len, dots } | Op::TieLen { len, dots } => {
                sum += beats(len.unwrap_or(l), *dots);
            }
```
置き換えた後:
```rust
            Op::Note { len, dots, .. }
            | Op::Rest { len, dots }
            | Op::TieLen { len, dots }
            | Op::ChordEnd { len, dots } => {
                sum += beats(len.unwrap_or(l), *dots);
            }
```

(g) `struct TrackState` の `last_note` を置き換える。

置き換える前:
```rust
    /// 直前の命令が音符だったとき、その音符の `notes` 内の index(タイの連結先)
    last_note: Option<usize>,
```
置き換えた後:
```rust
    /// 直前の命令が音符・和音だったとき、その音の `notes` 内の index(`^` と `&` の連結先。休符で空になる)
    last_notes: Vec<usize>,
    /// 直前の命令が和音だったか(和音は `&` でつなげない)
    last_chord: bool,
    /// 和音を始めたときのオクターブ(閉じで戻す)
    chord_octave: i32,
    /// 和音の中でここまでに読んだ音の (MIDI ノート番号, 元の文字位置)
    chord_notes: Vec<(i32, usize)>,
```

(h) `run_track` の `let mut st = TrackState { ... }` の `last_note: None,` を置き換える:

置き換える前: `        last_note: None,`
置き換えた後:
```rust
        last_notes: Vec::new(),
        last_chord: false,
        chord_octave: DEFAULT_OCTAVE,
        chord_notes: Vec::new(),
```

(i) `run_track` の `match op` の、`Op::Tie => {` から `Op::Note { ... }` の分岐の終わりまで(次の行が、`match` を閉じる `        }` と `    }` で、その後が `    if let Some(tp) = st.tie_pos {`)を、次のコード**全体**で置き換える(Python などで、`            Op::Tie => {` の位置から、`        }\n    }\n\n    if let Some(tp) = st.tie_pos {` の直前までを置き換える):

```rust
            Op::Tie => {
                if st.last_chord {
                    return Err(mml_err(pos, CHORD_TIE_MESSAGE));
                }
                if st.last_notes.is_empty() || st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の前に音符が必要です"));
                }
                st.tie_pos = Some(pos);
            }
            Op::TieLen { len, dots } => {
                if st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の直後には `^` を置けません"));
                }
                if st.last_notes.is_empty() {
                    return Err(mml_err(pos, "`^` の前に音符が必要です"));
                }
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                st.last_full += d;
                for &idx in &st.last_notes {
                    notes[idx].duration_sec = gated(st.last_full, st.last_gate);
                }
                st.t += d;
                check_limit(st.t, pos)?;
            }
            Op::Rest { len, dots } => {
                if let Some(tp) = st.tie_pos {
                    return Err(mml_err(tp, "`&` の後に音符が必要です"));
                }
                st.t += duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                st.last_notes.clear();
                st.last_chord = false;
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
                    let idx = st.last_notes[0];
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
                    st.last_notes.clear();
                    st.last_notes.push(notes.len() - 1);
                    st.last_chord = false;
                    st.last_full = d;
                    st.last_gate = st.gate;
                }
                st.t += d;
                check_limit(st.t, pos)?;
            }
            Op::ChordStart => {
                if let Some(tp) = st.tie_pos {
                    return Err(mml_err(tp, CHORD_TIE_MESSAGE));
                }
                st.chord_octave = st.octave;
                st.chord_notes.clear();
            }
            Op::ChordNote { semitone } => {
                st.chord_notes.push((12 * (st.octave + 1) + semitone, pos));
            }
            Op::ChordEnd { len, dots } => {
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                let velocity = f64::from(st.volume) / 15.0 / st.chord_notes.len() as f64;
                st.last_notes.clear();
                for &(midi, note_pos) in &st.chord_notes {
                    if notes.len() >= MAX_NOTES {
                        return Err(mml_err(note_pos, format!("音符は {MAX_NOTES} 個までです")));
                    }
                    notes.push(NoteEvent {
                        track,
                        start_sec: st.t,
                        duration_sec: gated(d, st.gate),
                        midi: midi as u8,
                        velocity,
                        timbre: st.timbre,
                    });
                    st.last_notes.push(notes.len() - 1);
                }
                st.last_chord = true;
                st.last_full = d;
                st.last_gate = st.gate;
                st.octave = st.chord_octave;
                st.t += d;
                check_limit(st.t, pos)?;
            }
```

(j) `fn run_track` のすぐ上(`/// 命令列を実行して…` のドキュメントコメントの直前)に、定数を追加する:

```rust
const CHORD_TIE_MESSAGE: &str = "`&` は和音とは組み合わせられません(長くしたいときは `^` を使ってください)";
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`(PR #511 / #513 までのテストと既存のテスト全部を含む)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|generates_frontend_bindings"`
Expected: 失敗なし。`git -C /home/onodai145/repos/github.com/onodai145/tsumugi status --short` に `tauri.gen.ts` が出ない。

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs 2>&1 | grep -c '^Diff in'
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML に和音 'ceg' を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
Expected: 差分の数が `5`(増えていない)。増えていたら、追加した部分だけを rustfmt の形に直してからコミットする(rustfmt が直す形は、`rustfmt --check` の出力に出る)。

---

### Task 2: フロント — 和音の色分け、入力補助キー、ヘルプ、ガイド

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlLexer.ts`、`mmlLexer.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlKeys.ts`、`mmlKeys.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/ui/settings/SoundChoicePicker.svelte`、`SoundChoicePicker.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/docs/guide/user-guide.md`

**Interfaces:**
- Consumes: `lexMml(text): MmlToken[]` の `push` / `digitsEnd` / `at` と `repeat` 種別、`MML_KEY_GROUPS` の `structure` グループ、`data-testid="mml-syntax-help"` の `<pre>`。
- Produces: `'` の開きは 1 つの `repeat` トークン、閉じの `'` は後ろの長さ・付点までを 1 つの `repeat` トークン。入力補助キー id `chord`(`'`)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlLexer.test.ts` の `describe("lexMml", ...)` の**中**(`t= は 1 つのコマンド…` の `it` の次)に追加する:

```ts
  it("和音は開きの ' と、閉じの ' とその直後の長さ・付点(repeat)。中は音符と < >", () => {
    expect(lex("'ceg'4.")).toEqual([
      ["'", "repeat"],
      ["c", "note"],
      ["e", "note"],
      ["g", "note"],
      ["'4.", "repeat"],
    ]);
    expect(lex("'c>c' d")).toEqual([
      ["'", "repeat"],
      ["c", "note"],
      [">", "command"],
      ["c", "note"],
      ["'", "repeat"],
      ["d", "note"],
    ]);
  });

  it("閉じていない ' は、以降を和音の中として読む(開きだけ repeat)", () => {
    expect(lex("'ce")).toEqual([
      ["'", "repeat"],
      ["c", "note"],
      ["e", "note"],
    ]);
  });
```

`frontend/src/lib/mmlKeys.test.ts` を 2 か所編集する。

(a) 「修飾は # - . & ^…」の `it` の、`structure` の期待を置き換える(`it` の題名の `[ ] : { } ;` も `[ ] : { } ' ;` にする):

置き換える前:
```ts
    expect(inserts("structure")).toEqual(["[", "]", ":", "{", "}", ";"]);
```
置き換えた後:
```ts
    expect(inserts("structure")).toEqual(["[", "]", ":", "{", "}", "'", ";"]);
```
題名: `"修飾は # - . & ^、オクターブは < >、構造は [ ] : { } ;"` → `"修飾は # - . & ^、オクターブは < >、構造は [ ] : { } ' ;"`

(b) 「キーだけで打った列…」の `it` の、キー列に和音を足す。

置き換える前:
```ts
      "tuplet-open", "c", "d", "tuplet-close", "len-4", "tie-len", "len-8",
      "track", "r", "len-4", "oct-down",
```
置き換えた後:
```ts
      "tuplet-open", "c", "d", "tuplet-close", "len-4", "tie-len", "len-8",
      "chord", "c", "e", "g", "chord", "len-4",
      "track", "r", "len-4", "oct-down",
```
題名の `{ c d } 4 ^ 8 ;` は `{ c d } 4 ^ 8 ' c e g ' 4 ;` にする。

`frontend/src/ui/settings/SoundChoicePicker.test.ts` の「記法ヘルプに、全トラックの初期テンポ t= の書き方が出る」の `it` の次に追加する:

```ts
  it("記法ヘルプに、和音 'ceg' の書き方が出る", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    expect(text).toContain("'ceg'4");
    expect(text).toContain("和音");
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | grep -E "×|Test Files|Tests |Error"`
Expected: 新しい `it`(和音の字句解析 2 つ、キーの 2 つ、ヘルプ)が FAIL。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlLexer.ts`。

- `lexMml` の、ループの手前の宣言を置き換える。

置き換える前:
```ts
  let i = 0;
  while (i < syms.length) {
```
置き換えた後:
```ts
  let i = 0;
  // 和音 `'…'` の中にいるか(開きと閉じの判定)。閉じていない `'` は、文末まで和音の中として読む
  let inChord = false;
  while (i < syms.length) {
```

- `;` の分岐の**直前**に、`'` の分岐を足す。

置き換える前:
```ts
    } else if (ch === ";") {
```
置き換えた後:
```ts
    } else if (ch === "'") {
      if (!inChord) {
        push(i, i + 1, "repeat");
        inChord = true;
        i++;
      } else {
        // 閉じの `'` は、後ろの長さと付点まで、1 つのトークンにする
        let j = digitsEnd(i + 1);
        while (at(j) === ".") j++;
        push(i, j, "repeat");
        inChord = false;
        i = j;
      }
    } else if (ch === ";") {
```

(b) `frontend/src/lib/mmlKeys.ts` の `structure` グループの `tuplet-close` の次に追加する:

```ts
      { id: "chord", label: "'", insert: "'", aria: "和音の始め・終わり '" },
```

(c) `frontend/src/ui/settings/SoundChoicePicker.svelte` の「記法」の `<pre>` の、`{cde}4 連符…` の行の次に、1 行足す。`<` と `>` は `&lt;` と `&gt;` で書く(Svelte の `<pre>` の中で、既存の行と同じ扱い):

```
'ceg'4 和音(同時に鳴らす。長さは閉じの ' の後ろに 1 つ。中は音符と &lt; &gt; だけ。音量は音数で割る。& とは組み合わせ不可、延ばすには ^)
```

(d) `docs/guide/user-guide.md` を編集する(Python などで、置き換え前の文字列がちょうど 1 回出ることを確かめながら置き換える)。

1. 記法表の連符 `{cde}4` の行の次に、次の 1 行を足す:

```markdown
| `'ceg'4.` | 和音。単引用符で囲んだ音を同時に鳴らす。長さと付点は閉じの `'` の後ろに 1 つ（和音全体の長さ。省略すると `l` の値）。中に書けるのは音符（`#` `+` `-` つき）と `<` `>` だけ（`'c>c'` はオクターブ違いの和音で、`<` `>` は和音の間だけ効き、後ろは元のオクターブに戻る）。各音の音量は `v` の音量を音数で割る（`'ceg'` の各音は 1/3。和音は単音より小さく聞こえる）。`^` は和音の全部の音を延ばす（`'ceg'4^4`）。`&` とは組み合わせられない。音の数は、和音の各音を 1 つずつ数える |
```

2. 入力補助キーの列挙を置き換える:

置き換える前:
```
`[` `]` `:` `{` `}` `;` を押すと
```
置き換えた後:
```
`[` `]` `:` `{` `}` `'` `;` を押すと
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts src/ui/settings/SoundChoicePicker.keys.test.ts src/ui/settings/mmlEditorExtensions.test.ts 2>&1 | grep -E "×|Test Files|Tests |Error"`
Expected: すべて PASS(`SoundChoicePicker.keys.test.ts` は新しい `'` キーも自動で押して確かめる)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -4`
Expected: 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件だけは出てよい)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts frontend/src/lib/mmlKeys.ts frontend/src/lib/mmlKeys.test.ts frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md && git commit -m "feat: MML の和音を色分けし、入力補助キー・ヘルプ・ガイドに追記(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 全体の確認と実機の確認

**Files:**
- 変更なし(確認だけ。直す必要が出たら、該当タスクの続きとして直す)。確認の記録を spec の末尾に追記する場合のみ、`docs/superpowers/specs/2026-10-20-issue-499-mml-chord-design.md` を編集する。

- [ ] **Step 1: 自動テストをすべて走らせる**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|panicked" | head
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -3
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm test 2>&1 | grep -E "Test Files|Tests "
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short
```
Expected: `cargo test` は失敗 0(`#[ignore]` は除く)。`pnpm check` は 0 errors。`pnpm test` は全件 PASS。`git status` が空(`tauri.gen.ts` が変わっていない)。

- [ ] **Step 2: 実アプリで確認する(隔離した Xvfb のインスタンス)**

実画面に出さず、ユーザーの本物のデータに触れない。メモリの `feedback-dev-server-verification-must-use-virtual-display.md` / `tauri-single-instance-blocks-xdg-isolated-verification.md` / `reference-isolated-instance-needs-short-xdg-path.md` / `debug-bridge-does-not-support-await.md` を先に読み、PR #511 / #513 の確認(spec の「実装後の確認」)と同じ方法で行う:
- 起動: `Xvfb :<空き番号>`、`dbus-run-session`、`XDG_*` を `/tmp/<短い名前>` 配下(小さなファイルだけ。大きなものは置かない)、`WAYLAND_DISPLAY` は unset、`GDK_BACKEND=x11`、`target/debug/tsumugi` を直接起動(ユーザーの `cargo tauri dev` の vite(127.0.0.1:5173)が作業ツリーを配信していれば、`cargo build` も `pnpm build` も要らない。バイナリが最新か、新しいエラー文言が入っているかを `grep -c -a` で確かめる)。起動した全プロセスに、一意なトークンを環境変数で付ける。
- アカウント: 起動前に `$XDG_CONFIG_HOME/com.onodai.tsumugi/settings.json` へ、ダミーのアカウント(`{"accounts":[{"id":"00000000-0000-4000-8000-000000000499","host":"misskey.invalid","username":"verify","userId":"verifyuser","displayName":"verify","avatarUrl":null}]}`)だけを書く。
- 画面: 投稿欄の「メニュー」→「設定」→「通知」→「通知音を鳴らす」ON → 種類「MML（打ち込み）」(ドロップダウンの項目は `[role=listbox] button`)。入力は `xdotool type`(エディタに `.focus()` したあと)。画面は `import -window root` で撮って自分で見る。

確認すること:
1. `'ceg'4 'dfa'4 'ceg'2` で、構文エラーが出ず、「再生時間 2.0 秒」・音 9/3000・トラック 1/4。`'` と、閉じの `'4` `'2` が色分けされる(構造の色)。
2. `'ce`(閉じていない): 位置つきのエラー(「1 文字目: 閉じていない `'` があります」)で、試聴と保存が `disabled`。「1 文字目」を押すと、カーソルが `'` の手前(添字 0)に移る。
3. `c&'ceg'`(2 文字目)、`'cr'`(3 文字目)、`''`(1 文字目)、`c 'c4e'`(5 文字目)で、それぞれ位置つきのエラーが出て、保存・試聴が `disabled` になる。
4. 「記法」を開くと、`'ceg'4 和音(…)` の行が出る(`<` `>` が文字として出ている)。
5. 手順 1 の MML で「試聴」を押し、`play_notify_sound` に `data:audio/wav;base64,…` が渡る(bridge で `fetch` の `ipc://` を包んで記録する。`window.__TAURI_INTERNALS__.invoke` は書き換えられない)。**音が鳴ること、和音の聞こえ方は、Xvfb に音声デバイスが無いので確認できない**。そう書く。

確認が終わったら、自分で起動した Xvfb / `dbus-run-session` / アプリ / bridge のプロセスを、トークンで特定した PID を指定して止める(`pkill` / `killall` は使わない)。`/tmp` に作った隔離ホームと、Xvfb のロックファイルも片付ける。ユーザー自身の `cargo tauri dev` のプロセスには触れない。

- [ ] **Step 3: 結果を spec の末尾に記録する**

`docs/superpowers/specs/2026-10-20-issue-499-mml-chord-design.md` の末尾に、次の節を追記する(実際に確認したことと、確認できなかったことを、事実のとおりに書く。数値と所要時間はこの手順で得た実測値を入れる。括弧書きのまま残さない):

```markdown
## 実装後の確認

(確認した日付・ブランチ・HEAD のコミット、隔離インスタンスの起動方法を書く。)

確認できたこと:
- 自動テスト: `cargo test` の結果、`pnpm check` の結果、`pnpm test` の結果(件数)。
- `rustfmt --check src/mml.rs` の差分の数(`main` と同じ 5 かどうか)。
- 3000 音ちょうどの和音と、ループで 3000 を超える入力(`chord_counts_each_note_toward_the_limit`)の所要時間。
- 実アプリ(隔離インスタンス): Step 2 の 1〜5 のそれぞれで、何を見て、何が起きたか。

確認できなかったこと:
- 実際に音が鳴ること、和音の聞こえ方(音量を音数で割った聞こえ方を含む。Xvfb に音声デバイスが無い)。
- Android の実機・エミュレーター、macOS / Windows。
```

- [ ] **Step 4: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add docs/superpowers/specs/2026-10-20-issue-499-mml-chord-design.md && git commit -m "docs: MML の和音の実装後の確認を記録(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: ブランチの扱いを決める(ユーザーに確認する)**

`superpowers:finishing-a-development-branch` に従う。PR を作る場合は、`.github/pull_request_template.md` の構成に手で合わせ、本文に `Refs #499` と書く(`Fixes` / `Closes` は書かない。#499 の「中くらい」の候補が未着手で残るため)。`gh pr merge` は `--merge`(squash にしない)。push 後に CI を待つループは作らない(ユーザーが見る)。#499 のチェックボックス(「和音の記法」)を更新するかは、ユーザーに確認してから行う。作業ブランチの削除も、ユーザーが頼んだときだけ行う。
