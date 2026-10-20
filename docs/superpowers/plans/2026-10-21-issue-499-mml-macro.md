# MML のマクロ `$name={…}` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に引数なしの文字列マクロ(`$riff={c d e f}` で定義し、`$riff` で展開する)を足し、字句解析・トラック数の数え方・入力補助キー・「記法」ヘルプ・ガイドを追随させる。

**Architecture:** `src-tauri/src/mml.rs` だけを変える(`NoteEvent` / `MmlScore` / `MmlError` / レンダラーは変えない)。`symbols` を「コメントを取り除く」`strip_comments` と「空白を取り除いて小文字化する」`finish_symbols` に分け、その間にマクロの前処理 `expand_macros` を入れる。展開された文字は元の位置を持ったままなので、パーサーの本体(`parse_track` / `run_track`)は変わらない。フロントは、定義を見つける共有の補助関数 `mmlMacros.ts` を足し、字句解析・`countTracks`・入力補助キー・ヘルプ・ガイドを足す。

**Tech Stack:** Rust(`src-tauri`、`cargo test`)、Svelte 5 + TypeScript(`frontend`、Vitest + jsdom)。

**Spec:** `docs/superpowers/specs/2026-10-20-issue-499-mml-macro-design.md`(前提: `2026-10-20-issue-499-mml-syntax-extension-design.md` / `…-shared-tempo-design.md` / `…-chord-design.md`)

## Global Constraints

- 作業ブランチは `feat/issue-499-mml-macro`(作成済み)。`main` には触らない。リポジトリルートは `/home/onodai145/repos/github.com/onodai145/tsumugi`。コマンドは絶対パスで書く(作業ディレクトリが変わっても動くように)。
- v1 と PR #511 / #513 / #514 / #516 の記法・既定値・範囲は変えない。今まで通っていた MML は、同じ `MmlScore` になる(`$` は今は構文エラーなので、保存済みの MML の意味は変わらない)。`$` を含まない MML は、`expand_macros` が何もせずにそのまま返す(速い経路)。
- `NoteEvent` / `MmlScore` / `MmlError` / `Timbre` と `mmlRender.ts` は変えない。`frontend/src/bindings/tauri.gen.ts` は変わらない(変わったら何かがおかしい)。
- 上限は 5000 文字(コメント・定義も含む)・3000 音・4 トラック・90 秒・展開後 `MAX_EXPANDED_OPS = 500_000` 命令(トラックごと)。値は変えない。新設: マクロ展開後 `MAX_EXPANDED_SYMBOLS = 100_000` 文字(Rust のみ。フロントには持たない)。
- マクロの仕様(spec が正):
  - 定義 `$名前 = { 本体 }`、展開 `$名前`。名前は `$` の直後に**途切れなく**続く `[A-Za-z0-9_]+`(小文字として扱う)。名前の直後に英数字か `_` が続くときは、空白かコメントで区切る(`$a c`。`$ac` は名前 `ac`)。記号の前は区切らなくてよい。
  - 定義は、名前の次(空白をはさんでよい)が `=`、その次(空白をはさんでよい)が `{` のときだけ。本体は、対になる `}` まで(本体の中の `{ }` は深さで数える)。定義はどこに書いてもよく、全トラックで使える(使う前に書かなくてもよい)。
  - 本体の中の `;` と定義はエラー。二重定義はエラー。再帰(直接・間接)はエラー。未定義はエラー(名前の先頭が定義済みの名前と一致するときは、空白で区切るよう案内する)。展開後 100,000 文字を超えたらエラー。
  - 展開された文字は元の位置を持つので、本体の誤りは**定義の中のその文字**を指す。エラー位置は元のテキストの文字位置(Unicode スカラ値の添字、0 始まり)。UI は 1 始まりの「N 文字目」。`MmlError.position` は `u32`(テストで `usize` と比べるときは `as usize`)。
- コミットメッセージは件名のみ(本文・箇条書きなし)。ハーネスの指示どおり、末尾に `Co-Authored-By` トレーラーを付ける(下の例は `-m` を 2 回に分けている)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら(GPG の署名がタイムアウトすることがある)、止めてユーザーに報告する(再試行しない)。
- Rust のテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::<名前>`。単一のフロントテストは `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run <パス>`(`pnpm test -- <パス>` はパスを無視して全件走る)。
- `rustfmt --check --edition 2021 src/mml.rs` には、`main` にも既にある 5 箇所の差分がある(サンプルと音色のテスト)。確認は「差分の数が増えていない(5 のまま)」で行い、既存の部分は整形しない(`cargo fmt` を全体に掛けない)。増えたら、追加した部分だけを rustfmt の形に直す(`rustfmt --check` の出力に、直す形が出る)。台帳に `Ruling:` を残す。テストの中の「行末コメントの直後にコメント行」は rustfmt がずらすので、行末コメントを使わない。
- **`/tmp` はメモリ上の tmpfs(16GB)。** 大きなもの(cargo のビルド出力、worktree)を `/tmp` に置かない。最終レビューに検証用のビルドをさせるときは、worktree もビルド出力もディスク上(`/home/onodai145/.cache/review-…/`)に置かせ、`CARGO_BUILD_JOBS=4` と `CARGO_PROFILE_DEV_DEBUG=0` を付け、終わったら正確なパスで片付けさせる。
- 直接ビルドしたデバッグバイナリは `frontend/dist` を埋め込む(ページの URL が `tauri://localhost`)。実機確認の前に、`cd frontend && pnpm build` のあとに `cd src-tauri && cargo build` をする(dist は git の管理外。どちらも数分かかる)。
- PR を作るときは、本文に `Fixes #499` / `Closes #499` を書かない(#499 には候補が未着手で残る)。`Refs #499` と書く。Issue のチェックボックスを更新するのは、ユーザーの確認を取ってから。
- ユーザーに見せる文章(最終報告・質問)は日本語。

## Review Focus

仕様が黙っているが、使う人が踏みやすい入力と条件。それぞれ、持ち主のタスクにテストを足してある。

1. 展開の暴走と深さ: 入れ子の爆発が 100,000 文字の上限で素早く止まること。再帰(直接・間接)の検出。300 段の連鎖(`$m299` → … → `$m0`)が、スタックを使い切らずに展開できること(`parse_mml` は tokio のワーカースレッドで動く)。5000 文字の入力で作れる最悪の入れ子の深さでの所要時間とスタックの深さ。(Task 1)
2. 名前の区切り: `$a c` / `$ac` / `$a/**/c` / `$a\nd` / `$a;d` / `[$a]2`、タブ、行末、`$a` と `$ab` の同時定義、`$at=60` と `$a t=60`、大文字、`$` 単独。(Task 1)
3. 定義の検出: `=` と `{` の前後の空白、`$a=c`(定義でない)、本体の波括弧の入れ子(連符 `{c d e}4`)、閉じていない `{`、本体の中の `;` と定義、本体の中の `$b`(使い)が定義と誤認されないこと(`$b` の直後が `}` のとき)。定義の閉じの `}` の後ろの数字。(Task 1、Task 2)
4. 位置: 本体の誤りが定義の中を指す。コメント・空白・絵文字(サロゲートペア)を含む入力で、位置が元のテキストのまま。展開後の `t=` / 和音 / 連符 / ループとの組み合わせ。(Task 1)
5. 既存の MML が同じ結果になること: `symbols` を `strip_comments` + `finish_symbols` に分けるリファクタ(コメントの位置に空白 1 つを残す変更が、`$` を含まない MML の結果を変えないこと。`c/**/d`、`//` 行末、CRLF)。既存テストが全部そのまま通る。(Task 1)
6. トラック数: 定義だけの区間がトラックに数えられないこと。Rust と JS(`countTracks`)の一致。本体の `;` はエラー。(Task 1、Task 2)
7. フロント: 字句解析と Rust が、何を定義と見なすかが一致すること(`$a t=60`、`$at=60`、`$a=c`)。定義の閉じの `}` が後ろの数字を取り込まず、本体の中の連符の `}4` は取り込むこと。閉じていない定義の色分け。入力補助キー `$`。(Task 2)

---

### Task 1: Rust — マクロの定義と展開

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/mml.rs`(import、`symbols` の置き換え、マクロの前処理の追加、`parse`、tests)

**Interfaces:**
- Consumes: `mml_err(pos, msg)`、`MmlError`、`parse` の `symbols(text)?` の 1 行。
- Produces: `type Sym = (char, usize)`、`fn strip_comments(text: &str) -> Result<Vec<Sym>, MmlError>`、`fn finish_symbols(chars: &[Sym]) -> Vec<Sym>`、`fn expand_macros(chars: Vec<Sym>) -> Result<Vec<Sym>, MmlError>`、`const MAX_EXPANDED_SYMBOLS: usize = 100_000`(`symbols` は無くなる)。

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の末尾(最後の `}` の直前)に追加する:

```rust
    #[test]
    fn macros_define_and_expand() {
        assert_eq!(
            must_parse("$riff={c d e f} $riff $riff"),
            must_parse("cdefcdef")
        );
        // 定義は、使う前でも後ろでもよい
        assert_eq!(must_parse("$riff $riff={c d}"), must_parse("cd"));
        // 入れ子
        assert_eq!(must_parse("$a={$b $b} $b={c} $a"), must_parse("cc"));
        // 空白・大文字、定義の `=` と `{` の前後の空白
        assert_eq!(must_parse("$RIFF = { C D } $riff"), must_parse("cd"));
        // 展開の直後の `4` は、本体の最後の音符の長さ(文字の置き換えなので)
        assert_eq!(
            must_parse("$riff={c d e f} $riff 4"),
            must_parse("c d e f4")
        );
        // 本体の中の波括弧(連符)は、対で数える
        assert_eq!(must_parse("$u={{c d e}4} $u"), must_parse("{c d e}4"));
        // 本体に、状態を変えるコマンド・ループ・和音を書ける
        assert_eq!(
            must_parse("$p={o5 l8 [c d]2} $p"),
            must_parse("o5 l8 [c d]2")
        );
        assert_eq!(must_parse("$q={'ceg'4} $q"), must_parse("'ceg'4"));
        // 本体の中から、別のマクロを使える(使いの直後が空白でも `}` でもよい)
        assert_eq!(must_parse("$a={c} $b={$a d} $b"), must_parse("cd"));
        // 同じマクロを、複数のトラックで使える
        let s = must_parse("$m={c d} $m ; $m");
        assert_eq!(s.track_count, 2);
        assert_eq!(s.notes.len(), 4);
    }

    #[test]
    fn macro_names_end_at_the_first_non_name_character() {
        // 空白(かコメント)で区切る。記号の前は、区切らなくてよい
        assert_eq!(must_parse("$a={c} $a d"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a/**/d"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a\nd"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a\td"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a;d"), must_parse("c;d"));
        assert_eq!(must_parse("$a={c} [$a]2"), must_parse("[c]2"));
        // `$a` と `$ab` は別のマクロ
        assert_eq!(must_parse("$a={c} $ab={d} $a $ab"), must_parse("cd"));
        // `$a t=60` は `$a` の展開と `t=60`。`$at=60` は名前 `at`
        assert_eq!(must_parse("$a={o5} $a t=60 c"), must_parse("o5 t=60 c"));
        let e = must_fail("$a={o5} $at=60 c");
        assert_eq!(e.position, 8);
        assert!(
            e.message.contains("`$a`") && e.message.contains("`t`"),
            "{}",
            e.message
        );
        // 区切らないと名前の一部になる
        let e = must_fail("$a={c} $ad");
        assert_eq!(e.position, 7);
        assert!(e.message.contains("空白で区切って"), "{}", e.message);
    }

    #[test]
    fn definition_only_sections_are_not_tracks() {
        let s = must_parse("$a={c} ; d");
        assert_eq!(s.track_count, 1);
        assert_eq!(s.notes.len(), 1);
        assert_eq!(must_parse("$a={c} $a ; $a").track_count, 2);
        // 本体の `t=` は、展開された場所の規則で検査される
        assert_eq!(
            must_parse("$t={t=60} $t c ; d"),
            must_parse("t=60 c ; d")
        );
    }

    #[test]
    fn invalid_macros() {
        let pos = |t: &str| must_fail(t).position;
        assert_eq!(pos("c $zzz"), 2); // 未定義
        assert_eq!(pos("c $"), 2); // 名前が無い
        assert_eq!(pos("$a=c"), 0); // `={` の形でなければ定義ではなく、未定義の展開
        assert_eq!(pos("$a={c d"), 3); // 閉じていない
        assert_eq!(pos("$a={c;d} $a"), 5); // 本体の `;`
        assert_eq!(pos("$a={$b={c}}"), 4); // 本体の中の定義
        assert_eq!(pos("$a={c} $a={d}"), 7); // 二重定義
        assert_eq!(pos("$a={$a} $a"), 4); // 再帰(本体の中の `$`)
        assert_eq!(pos("$a={$b} $b={$a} $a"), 12); // 間接の再帰
        assert_eq!(pos("$a={o9} $a"), 5); // 本体の誤りは、定義の中の文字を指す
        assert_eq!(pos("/* あ */ $zzz"), 8); // コメントの後ろでも、元の位置
        assert!(must_fail("$a={c} $ab").message.contains("空白で区切って"));
        assert!(must_fail("c $zzz").message.contains("未定義"));
        assert!(must_fail("$a={$a} $a").message.contains("自分自身"));
        assert!(must_fail("c $").message.contains("名前"));
    }

    #[test]
    fn macro_expansion_is_capped_and_deep_chains_work() {
        // 1000 文字の本体を 101 回並べたマクロ: 展開後 10 万文字を超える
        let big = format!("$a={{{}}} $b={{{}}} $b", "c".repeat(1000), "$a ".repeat(101));
        assert!(big.chars().count() <= MAX_TEXT_CHARS);
        let started = std::time::Instant::now();
        let e = must_fail(&big);
        assert!(e.message.contains("100000"), "{}", e.message);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        // 入れ子が深い連鎖(300 段)は、スタックを使い切らずに展開できる
        let mut chain = String::from("$m0={c} ");
        for i in 1..300 {
            chain.push_str(&format!("$m{i}={{$m{} }} ", i - 1));
        }
        chain.push_str("$m299");
        assert!(chain.chars().count() <= MAX_TEXT_CHARS);
        assert_eq!(must_parse(&chain), must_parse("c"));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests:: 2>&1 | grep -E "^test .*FAILED|test result"`
Expected: 上で足した 5 つが FAIL(`$` は今は「不明な文字」)。既存のテストは PASS のまま。

- [ ] **Step 3: 実装する**

(a) ファイル先頭の `use crate::error::Error;` の次の行に追加する:

```rust
use std::collections::HashMap;
```

(b) `symbols` を置き換える。`/// 空白とコメントを除いた (ASCII だけ小文字化した文字, 元の文字位置) の列を作る。` で始まるドキュメントコメントの行から、`fn symbols` を閉じる `}` の行(その関数の次の、行頭の `}`)までを、次のコード**全体**で置き換える(Python などで、開始の行と終わりの行の位置を探して置き換える):

```rust
/// (文字, 元の文字位置)。
type Sym = (char, usize);

/// コメントを取り除いた (文字, 元の文字位置) の列を作る。空白はそのまま残す(マクロの名前の区切りに使う)。
/// コメントは `// …`(改行の手前まで)と `/* … */`(入れ子にできない)で、開始位置の空白 1 つに置き換える
/// (コメントも名前の区切りとして働く)。`/` だけはコメントにならず、そのまま列に残って `parse_track` が
/// 「不明な文字」にする。`;` はトラック区切りなのでコメントの開始にしない。
/// 閉じていない `/*` は、その `/*` の位置のエラー。
fn strip_comments(text: &str) -> Result<Vec<Sym>, MmlError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            out.push((' ', i));
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let open = i;
            out.push((' ', open));
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
        out.push((c, i));
        i += 1;
    }
    Ok(out)
}

/// 空白を除き、ASCII だけ小文字化する(パーサーが読む形)。
fn finish_symbols(chars: &[Sym]) -> Vec<Sym> {
    chars
        .iter()
        .filter(|(c, _)| !c.is_whitespace())
        .map(|&(c, p)| (c.to_ascii_lowercase(), p))
        .collect()
}

/// マクロを展開した後の文字数の上限。入れ子のマクロで展開が膨らむ入力を抑える。
const MAX_EXPANDED_SYMBOLS: usize = 100_000;

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `chars[i]`(`$`)の直後に途切れなく続く名前(小文字にしたもの)と、その次の添字を返す。
fn read_macro_name(chars: &[Sym], i: usize) -> (String, usize) {
    let mut j = i + 1;
    let mut name = String::new();
    while j < chars.len() && is_name_char(chars[j].0) {
        name.push(chars[j].0.to_ascii_lowercase());
        j += 1;
    }
    (name, j)
}

/// `from` から、空白を読み飛ばした次の添字。
fn skip_whitespace(chars: &[Sym], mut from: usize) -> usize {
    while from < chars.len() && chars[from].0.is_whitespace() {
        from += 1;
    }
    from
}

/// `chars[i]`(`$`)が定義の頭(`$名前 = {`)なら、名前と `{` の添字を返す。
fn definition_head(chars: &[Sym], i: usize) -> Option<(String, usize)> {
    let (name, j) = read_macro_name(chars, i);
    if name.is_empty() {
        return None;
    }
    let eq = skip_whitespace(chars, j);
    if chars.get(eq).map(|s| s.0) != Some('=') {
        return None;
    }
    let open = skip_whitespace(chars, eq + 1);
    if chars.get(open).map(|s| s.0) != Some('{') {
        return None;
    }
    Some((name, open))
}

/// 定義(`$名前 = { 本体 }`)を取り除き、表に入れる。戻り値は (定義を除いた列, 名前 → 本体)。
fn collect_macros(chars: &[Sym]) -> Result<(Vec<Sym>, HashMap<String, Vec<Sym>>), MmlError> {
    let mut rest = Vec::with_capacity(chars.len());
    let mut macros: HashMap<String, Vec<Sym>> = HashMap::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].0 == '$' {
            if let Some((name, open)) = definition_head(chars, i) {
                if macros.contains_key(&name) {
                    return Err(mml_err(
                        chars[i].1,
                        format!("マクロ `${name}` は既に定義されています"),
                    ));
                }
                // 本体は、対になる `}` まで(本体の中の `{ }` は深さで数える)
                let mut depth = 1usize;
                let mut k = open + 1;
                while k < chars.len() {
                    match chars[k].0 {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        ';' => {
                            return Err(mml_err(chars[k].1, "マクロの中には `;` を書けません"));
                        }
                        '$' if definition_head(chars, k).is_some() => {
                            return Err(mml_err(chars[k].1, "マクロの中には定義を書けません"));
                        }
                        _ => {}
                    }
                    k += 1;
                }
                if k >= chars.len() {
                    return Err(mml_err(
                        chars[open].1,
                        "マクロの本体の `{` が閉じていません",
                    ));
                }
                macros.insert(name, chars[open + 1..k].to_vec());
                i = k + 1;
                continue;
            }
        }
        rest.push(chars[i]);
        i += 1;
    }
    Ok((rest, macros))
}

/// 未定義のマクロのエラー。名前の先頭が定義済みの名前と一致するときは、区切りを案内する
/// (`$ab` は `$a` + `b` ではなく、名前 `ab` と読まれる)。
fn undefined_macro(pos: usize, name: &str, macros: &HashMap<String, Vec<Sym>>) -> MmlError {
    if name.is_empty() {
        return mml_err(pos, "`$` の後にマクロの名前が必要です");
    }
    let known = macros
        .keys()
        .filter(|k| name.starts_with(k.as_str()))
        .max_by_key(|k| k.len());
    match known {
        Some(k) => mml_err(
            pos,
            format!(
                "未定義のマクロ `${name}` です(`${k}` を使うなら、後ろの `{}` を空白で区切ってください)",
                &name[k.len()..]
            ),
        ),
        None => mml_err(
            pos,
            format!("未定義のマクロ `${name}` です(定義は `$名前={{…}}` の形で書きます)"),
        ),
    }
}

/// `chars` の `$名前` を、本体で置き換えながら `out` に写す(本体の中の `$` も再帰的に展開する)。
/// `stack` は展開中のマクロの名前(再帰の検出用)。`use_pos` は、いま展開している `$` の位置(上限のエラーの位置)。
fn expand_into(
    chars: &[Sym],
    macros: &HashMap<String, Vec<Sym>>,
    stack: &mut Vec<String>,
    out: &mut Vec<Sym>,
    use_pos: usize,
) -> Result<(), MmlError> {
    let mut i = 0;
    while i < chars.len() {
        let (c, pos) = chars[i];
        if c == '$' {
            let (name, j) = read_macro_name(chars, i);
            let Some(body) = macros.get(&name) else {
                return Err(undefined_macro(pos, &name, macros));
            };
            if stack.contains(&name) {
                return Err(mml_err(
                    pos,
                    format!("マクロ `${name}` が自分自身を呼んでいます"),
                ));
            }
            stack.push(name);
            expand_into(body, macros, stack, out, pos)?;
            stack.pop();
            i = j;
            continue;
        }
        if out.len() >= MAX_EXPANDED_SYMBOLS {
            return Err(mml_err(
                use_pos,
                format!("マクロを展開した文字数が {MAX_EXPANDED_SYMBOLS} を超えました"),
            ));
        }
        out.push((c, pos));
        i += 1;
    }
    Ok(())
}

/// マクロの定義を取り除き、`$名前` を本体で置き換える。`$` を含まない MML は、そのまま返す。
fn expand_macros(chars: Vec<Sym>) -> Result<Vec<Sym>, MmlError> {
    if !chars.iter().any(|s| s.0 == '$') {
        return Ok(chars);
    }
    let (rest, macros) = collect_macros(&chars)?;
    let mut out = Vec::with_capacity(rest.len());
    expand_into(&rest, &macros, &mut Vec::new(), &mut out, 0)?;
    Ok(out)
}
```

(c) `parse` の中の、`symbols` を呼ぶ行を置き換える。

置き換える前:
```rust
    let syms = symbols(text)?;
```
置き換えた後:
```rust
    let syms = finish_symbols(&expand_macros(strip_comments(text)?)?);
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: `test result: ok.`(PR #516 までのテストと既存のテスト全部を含む。コメントのテスト `block_comments_may_span_lines…` `error_positions_after_comments…` など、`strip_comments` に変えた部分も通る)。`macro_expansion_is_capped_and_deep_chains_work` の所要時間(`cargo test --lib mml::tests::macro_expansion 2>&1 | grep "finished in"`)を控える。**1 秒を超えたら、先へ進まず、ユーザーに所要時間を報告する。**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|generates_frontend_bindings"`
Expected: 失敗なし。`git -C /home/onodai145/repos/github.com/onodai145/tsumugi status --short` に `tauri.gen.ts` が出ない。

- [ ] **Step 5: 整形を確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs 2>&1 | grep -c '^Diff in'
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/mml.rs && git commit -m "feat: MML にマクロ \$name={…} を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
Expected: 差分の数が `5`(増えていない)。増えていたら、追加した部分だけを rustfmt の形に直してからコミットする(コミットの `-m` の `\$` は、シェルで `$` を文字として渡すための書き方)。

---

### Task 2: フロント — 定義の検出、字句解析、トラック数、入力補助キー、ヘルプ、ガイド

**Files:**
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlMacros.ts`、`mmlMacros.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlLexer.ts`、`mmlLexer.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlUsage.ts`、`mmlUsage.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlKeys.ts`、`mmlKeys.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/ui/settings/SoundChoicePicker.svelte`、`SoundChoicePicker.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/docs/guide/user-guide.md`

**Interfaces:**
- Consumes: `blankComments(text): string`(`mmlComments.ts`)、`lexMml` の `push` / `at` / `digitsEnd` / `symbols`(`ch` は小文字化済み、`from`/`to` は元のテキストの添字)、`countTracks(text)`、`MML_KEY_GROUPS` の `structure` グループ、`data-testid="mml-syntax-help"` の `<pre>`。
- Produces: `MacroDefinition { name, from, to, bodyFrom, bodyTo }`、`findMacroDefinitions(text): MacroDefinition[]`、`blankMacroDefinitions(text): string`(`mmlMacros.ts`)。入力補助キー id `macro`(`$`)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlMacros.test.ts` を新規作成する:

```ts
import { describe, expect, it } from "vitest";
import { blankMacroDefinitions, findMacroDefinitions } from "./mmlMacros";

describe("findMacroDefinitions", () => {
  it("定義の範囲(`$` から閉じの `}` の次まで)と、本体の範囲を返す", () => {
    expect(findMacroDefinitions("$a={c}")).toEqual([{ name: "a", from: 0, to: 6, bodyFrom: 4, bodyTo: 5 }]);
  });

  it("本体の中の波括弧(連符)は対で数える", () => {
    expect(findMacroDefinitions("$u={{c d}4} x")).toEqual([{ name: "u", from: 0, to: 11, bodyFrom: 4, bodyTo: 10 }]);
  });

  it("`=` と `{` の前後の空白、大文字(名前は小文字にする)", () => {
    expect(findMacroDefinitions("$Riff = { c }")).toEqual([{ name: "riff", from: 0, to: 13, bodyFrom: 9, bodyTo: 12 }]);
  });

  it("定義でないもの: 展開、`$a t=60`、`$at=60`、`=` の次が `{` でない、名前が無い", () => {
    expect(findMacroDefinitions("$a")).toEqual([]);
    expect(findMacroDefinitions("$a t=60")).toEqual([]);
    expect(findMacroDefinitions("$at=60")).toEqual([]);
    expect(findMacroDefinitions("$a=c")).toEqual([]);
    expect(findMacroDefinitions("$ ={c}")).toEqual([]);
  });

  it("閉じていない定義は、文末まで", () => {
    expect(findMacroDefinitions("$a={c d")).toEqual([{ name: "a", from: 0, to: 7, bodyFrom: 4, bodyTo: 7 }]);
  });

  it("複数の定義と、本体の中の展開(定義と誤認しない)", () => {
    const defs = findMacroDefinitions("$a={$b $b} $b={c}");
    expect(defs.map((d) => d.name)).toEqual(["a", "b"]);
  });

  it("位置は UTF-16 の添字(サロゲートペアは 2)", () => {
    expect(findMacroDefinitions("😀 $a={c}")[0].from).toBe(3);
  });
});

describe("blankMacroDefinitions", () => {
  it("定義を空白にし、長さと、定義以外の添字を変えない", () => {
    const text = "$a={c} d";
    const out = blankMacroDefinitions(text);
    expect(out).toHaveLength(text.length);
    expect(out.trim()).toBe("d");
    expect(out[text.length - 1]).toBe("d");
  });

  it("定義が無ければ同じ文字列", () => {
    expect(blankMacroDefinitions("c4 $a d4")).toBe("c4 $a d4");
  });

  it("本体の中の `;` も空白になる", () => {
    expect(blankMacroDefinitions("c $a={x;y} d")).not.toContain(";");
  });
});
```

`frontend/src/lib/mmlUsage.test.ts` の `describe("countTracks", ...)` の**中**(最後の `it` の後)に追加する:

```ts
  it("マクロの定義だけの区間は数えない(パーサーは、定義を取り除いてからトラックに分ける)", () => {
    expect(countTracks("$a={c} ; d")).toBe(1);
    expect(countTracks("$a={c} $a ; $a")).toBe(2);
    expect(countTracks("// x\n$a={c d}")).toBe(0);
  });
```

`frontend/src/lib/mmlLexer.test.ts` の `describe("lexMml", ...)` の**中**(「コメントの間の空白だけの断片は捨てる…」の `it` の次)に追加する:

```ts
  it("マクロの定義の頭 `$名前=` と展開 `$名前` は command。本体の中身は今までどおりの色", () => {
    expect(lex("$riff={c d}")).toEqual([
      ["$riff=", "command"],
      ["{", "repeat"],
      ["c", "note"],
      ["d", "note"],
      ["}", "repeat"],
    ]);
    expect(lex("$riff d")).toEqual([
      ["$riff", "command"],
      ["d", "note"],
    ]);
  });

  it("マクロの名前は、`$` の直後に途切れなく続く英数字と _(空白で終わる)", () => {
    expect(lex("$a c")).toEqual([
      ["$a", "command"],
      ["c", "note"],
    ]);
    expect(lex("$ac")).toEqual([["$ac", "command"]]);
    expect(lex("$a/**/c")).toEqual([
      ["$a", "command"],
      ["/**/", "comment"],
      ["c", "note"],
    ]);
    expect(lex("c $")).toEqual([
      ["c", "note"],
      ["$", "unknown"],
    ]);
  });

  it("定義の閉じの } は後ろの数字を取り込まない。本体の中の連符の }4 は取り込む", () => {
    expect(lex("$a={c}4")).toEqual([
      ["$a=", "command"],
      ["{", "repeat"],
      ["c", "note"],
      ["}", "repeat"],
      ["4", "unknown"],
    ]);
    expect(lex("$a={{c}4}")).toEqual([
      ["$a=", "command"],
      ["{", "repeat"],
      ["{", "repeat"],
      ["c", "note"],
      ["}4", "repeat"],
      ["}", "repeat"],
    ]);
  });

  it("`$a t=60` は展開と t=、`$at=60` は名前 at(定義ではない)", () => {
    expect(lex("$a t=60")).toEqual([
      ["$a", "command"],
      ["t=", "command"],
      ["60", "number"],
    ]);
    expect(lex("$at=60")[0]).toEqual(["$at", "command"]);
  });
```

`frontend/src/lib/mmlKeys.test.ts` を 2 か所編集する。

(a) 「修飾は # - . & ^…」の `it` の、`structure` の期待を置き換える。

置き換える前:
```ts
    expect(inserts("structure")).toEqual(["[", "]", ":", "{", "}", "'", ";"]);
```
置き換えた後:
```ts
    expect(inserts("structure")).toEqual(["[", "]", ":", "{", "}", "'", "$", ";"]);
```
題名の `[ ] : { } ' ;` は `[ ] : { } ' $ ;` にする。

(b) 「キーだけで打った列…」の `it` の、キー列に `$` を足す。

置き換える前:
```ts
      "chord", "c", "e", "g", "chord", "len-4",
      "track", "r", "len-4", "oct-down",
```
置き換えた後:
```ts
      "chord", "c", "e", "g", "chord", "len-4",
      "macro", "c",
      "track", "r", "len-4", "oct-down",
```
題名の `' c e g ' 4 ; r 4 <` は `' c e g ' 4 $ c ; r 4 <` にする。

`frontend/src/ui/settings/SoundChoicePicker.test.ts` の「記法ヘルプに、和音 'ceg' の書き方が出る」の `it` の次に追加する:

```ts
  it("記法ヘルプに、マクロ $riff の書き方と、名前の区切りの規則が出る", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    expect(text).toContain("$riff");
    expect(text).toContain("マクロ");
    expect(text).toContain("空白で区切る");
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlMacros.test.ts src/lib/mmlUsage.test.ts src/lib/mmlLexer.test.ts src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | grep -E "×|Test Files|Tests |Error"`
Expected: `mmlMacros.test.ts` は import 失敗(ファイルが無い)。`countTracks`・字句解析・キー・ヘルプの新しい `it` が FAIL。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlMacros.ts` を新規作成する:

```ts
// MML のマクロ(Issue #499)の定義 `$名前 = { 本体 }` を見つける。Rust の `mml.rs` の `collect_macros` と同じ規則
// (構文の検証は Rust 側の parse_mml)。範囲は元のテキストの添字(UTF-16)。コメントは、呼ぶ前に `blankComments` で
// 空白にしておく。名前は `$` の直後に途切れなく続く [A-Za-z0-9_]+、次(空白をはさんでよい)が `=` と `{` のときだけ定義。

export interface MacroDefinition {
  /** 小文字にした名前 */
  name: string;
  /** `$` の添字 */
  from: number;
  /** 閉じの `}` の次の添字(閉じていなければ文末) */
  to: number;
  /** 本体の始まり(`{` の次)の添字 */
  bodyFrom: number;
  /** 閉じの `}` の添字(閉じていなければ文末) */
  bodyTo: number;
}

const isNameChar = (ch: string): boolean => /^[A-Za-z0-9_]$/.test(ch);
const isSpace = (ch: string): boolean => /^\s$/u.test(ch);

/** 定義の範囲を、昇順で返す。本体の中の `$名前`(展開)は、定義としては数えない */
export function findMacroDefinitions(text: string): MacroDefinition[] {
  const skip = (from: number): number => {
    let k = from;
    while (k < text.length && isSpace(text[k])) k++;
    return k;
  };
  const out: MacroDefinition[] = [];
  let i = 0;
  while (i < text.length) {
    if (text[i] !== "$") {
      i++;
      continue;
    }
    let j = i + 1;
    while (j < text.length && isNameChar(text[j])) j++;
    if (j === i + 1) {
      i++;
      continue;
    }
    const eq = skip(j);
    if (text[eq] !== "=") {
      i = j;
      continue;
    }
    const open = skip(eq + 1);
    if (text[open] !== "{") {
      i = j;
      continue;
    }
    let depth = 1;
    let k = open + 1;
    while (k < text.length) {
      if (text[k] === "{") depth++;
      else if (text[k] === "}" && --depth === 0) break;
      k++;
    }
    const closed = k < text.length;
    const to = closed ? k + 1 : text.length;
    out.push({ name: text.slice(i + 1, j).toLowerCase(), from: i, to, bodyFrom: open + 1, bodyTo: k });
    i = to;
  }
  return out;
}

/** 定義を空白に置き換えた文字列。長さと、定義以外の各文字の添字は変わらない */
export function blankMacroDefinitions(text: string): string {
  const defs = findMacroDefinitions(text);
  if (defs.length === 0) return text;
  let out = "";
  let at = 0;
  for (const { from, to } of defs) {
    out += text.slice(at, from) + " ".repeat(to - from);
    at = to;
  }
  return out + text.slice(at);
}
```

(b) `frontend/src/lib/mmlUsage.ts` を編集する。

- import の `import { blankComments } from "./mmlComments";` の次の行に追加する: `import { blankMacroDefinitions } from "./mmlMacros";`
- `countTracks` を置き換える。

置き換える前:
```ts
export function countTracks(text: string): number {
  return blankComments(text)
    .split(";")
    .filter((seg) => seg.trim() !== "").length;
}
```
置き換えた後:
```ts
export function countTracks(text: string): number {
  return blankMacroDefinitions(blankComments(text))
    .split(";")
    .filter((seg) => seg.trim() !== "").length;
}
```
- `countTracks` のドキュメントコメント(`/** パーサーと同じく、コメントを除いてから …`)の「コメントを除いてから」を「コメントとマクロの定義を除いてから」にする。

(c) `frontend/src/lib/mmlLexer.ts` を編集する。

- import の `import { blankComments, findComments } from "./mmlComments";` の次の行に追加する: `import { findMacroDefinitions } from "./mmlMacros";`
- `lexMml` の先頭を置き換える。

置き換える前:
```ts
  const syms = symbols(blankComments(text));
```
置き換えた後:
```ts
  const blanked = blankComments(text);
  const syms = symbols(blanked);
  // マクロの定義の頭(`$`)と閉じ(`}`)の位置。定義の閉じの `}` は、後ろの長さ・付点を取り込まない
  const defs = findMacroDefinitions(blanked);
  const defStarts = new Set(defs.map((d) => d.from));
  const defCloses = new Set(defs.map((d) => d.bodyTo));
```
- `}` の分岐を置き換える。

置き換える前:
```ts
    } else if (ch === "}") {
      // `}` の後の長さと付点まで、1 つのトークンにする
      let j = digitsEnd(i + 1);
      while (at(j) === ".") j++;
      push(i, j, "repeat");
      i = j;
    } else if (ch === "'") {
```
置き換えた後:
```ts
    } else if (ch === "}") {
      if (defCloses.has(syms[i].from)) {
        // マクロの定義の閉じ。連符の `}` と違い、後ろの長さ・付点は取り込まない
        push(i, i + 1, "repeat");
        i++;
      } else {
        // `}` の後の長さと付点まで、1 つのトークンにする
        let j = digitsEnd(i + 1);
        while (at(j) === ".") j++;
        push(i, j, "repeat");
        i = j;
      }
    } else if (ch === "'") {
```
- `;` の分岐の**直前**に、`$` の分岐を足す。

置き換える前:
```ts
    } else if (ch === ";") {
```
置き換えた後:
```ts
    } else if (ch === "$") {
      // マクロ。名前は、元のテキストで `$` の直後に途切れなく続く [a-z0-9_]+(空白・コメントで終わる)
      let j = i + 1;
      while (j < syms.length && /^[a-z0-9_]$/.test(syms[j].ch) && syms[j].from === syms[j - 1].to) j++;
      if (j === i + 1) {
        push(i, i + 1, "unknown");
        i++;
      } else if (defStarts.has(syms[i].from)) {
        // 定義の頭 `$名前 =`(空白は syms に無いので、`=` は名前のすぐ後ろ)
        push(i, j + 1, "command");
        i = j + 1;
      } else {
        push(i, j, "command");
        i = j;
      }
    } else if (ch === ";") {
```

(d) `frontend/src/lib/mmlKeys.ts` の `structure` グループの `chord` の次に追加する:

```ts
      { id: "macro", label: "$", insert: "$", aria: "マクロ $" },
```

(e) `frontend/src/ui/settings/SoundChoicePicker.svelte` の「記法」の `<pre>` の、`'ceg'4 和音…` の行の次に、1 行足す。波括弧は `&#123;` `&#125;` で書く:

```
$riff=&#123;c d e f&#125; マクロ(文字の置き換え)。$riff で展開。定義はどこに書いてもよい。名前の直後に英数字が続くときは空白で区切る($riff 4)。本体に ; と定義は書けない
```

(f) `docs/guide/user-guide.md` を編集する(Python などで、置き換え前の文字列がちょうど 1 回出ることを確かめながら置き換える)。

1. 記法表の和音 `'ceg'4.` の行の次に、次の 1 行を足す:

```markdown
| `$riff={c d e f}`、`$riff` | マクロ(同じフレーズに名前を付けて、文字数を節約する)。`$名前={…}` で定義し、`$名前` で展開する(文字の置き換え)。定義はどこに書いてもよく、全トラックで使える(使う前に書かなくてもよい)。名前は英小文字・数字・`_` で、`$` の直後に途切れなく続く並びが名前になる(大文字は小文字として扱う)。**名前の直後に英数字か `_` が続くときは、空白かコメントで区切る**(`$riff 4` と書く。`$riff4` は名前 `riff4` になる)。`$a` と `$ab` は別のマクロ。本体にはその場に置いて意味が通る MML を書ける(本体の中の `{ }` は連符と同じく対で数える)。`;`(トラックの区切り)と定義は書けない。マクロの中から別のマクロを使えるが、再帰はエラー。展開後の文字数は 100,000 文字まで。本体の誤りは、定義の中の文字を指す。定義だけの区間(`$a={c} ; d` の `;` の前)は、トラックに数えない |
```

2. 入力補助キーの列挙を置き換える:

置き換える前:
```
`[` `]` `:` `{` `}` `'` `;` を押すと
```
置き換えた後:
```
`[` `]` `:` `{` `}` `'` `$` `;` を押すと
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlMacros.test.ts src/lib/mmlUsage.test.ts src/lib/mmlLexer.test.ts src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts src/ui/settings/SoundChoicePicker.keys.test.ts src/ui/settings/mmlEditorExtensions.test.ts 2>&1 | grep -E "×|Test Files|Tests |Error"`
Expected: すべて PASS(`SoundChoicePicker.keys.test.ts` は新しい `$` キーも自動で押して確かめる)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -4`
Expected: 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件だけは出てよい)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlMacros.ts frontend/src/lib/mmlMacros.test.ts frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts frontend/src/lib/mmlUsage.ts frontend/src/lib/mmlUsage.test.ts frontend/src/lib/mmlKeys.ts frontend/src/lib/mmlKeys.test.ts frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md && git commit -m "feat: MML のマクロを色分けし、トラック数・入力補助キー・ヘルプ・ガイドに追記(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 全体の確認と実機の確認

**Files:**
- 変更なし(確認だけ。直す必要が出たら、該当タスクの続きとして直す)。確認の記録を spec の末尾に追記する場合のみ、`docs/superpowers/specs/2026-10-20-issue-499-mml-macro-design.md` を編集する。

- [ ] **Step 1: 自動テストをすべて走らせる**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|panicked" | head
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -3
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm test 2>&1 | grep -E "Test Files|Tests "
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short
```
Expected: `cargo test` は失敗 0(`#[ignore]` は除く)。`pnpm check` は 0 errors。`pnpm test` は全件 PASS。`git status` が空(`tauri.gen.ts` が変わっていない)。

- [ ] **Step 2: 実アプリで確認する(隔離した Xvfb のインスタンス)**

実画面に出さず、ユーザーの本物のデータに触れない。メモリの `feedback-dev-server-verification-must-use-virtual-display.md` / `tauri-single-instance-blocks-xdg-isolated-verification.md` / `reference-isolated-instance-needs-short-xdg-path.md` / `debug-bridge-does-not-support-await.md` を先に読み、これまでの PR(spec の「実装後の確認」)と同じ方法で行う:
- ビルド: `cd frontend && pnpm build` のあとに `cd src-tauri && cargo build`(直接ビルドしたデバッグバイナリは `frontend/dist` を埋め込むため。バイナリに新しいエラー文言が入っているかを `grep -c -a` で確かめる)。
- 起動: `Xvfb :<空き番号>`、`dbus-run-session`、`XDG_*` を `/tmp/<短い名前>` 配下(小さなファイルだけ)、`WAYLAND_DISPLAY` は unset、`GDK_BACKEND=x11`、`target/debug/tsumugi` を直接起動。起動した全プロセスに、一意なトークンを環境変数で付ける。
- アカウント: 起動前に `$XDG_CONFIG_HOME/com.onodai.tsumugi/settings.json` へ、ダミーのアカウント(`{"accounts":[{"id":"00000000-0000-4000-8000-000000000499","host":"misskey.invalid","username":"verify","userId":"verifyuser","displayName":"verify","avatarUrl":null}]}`)だけを書く。
- 画面: 投稿欄の「メニュー」→「設定」→「通知」→「通知音を鳴らす」ON → 種類「MML（打ち込み）」(ドロップダウンの項目は `[role=listbox] button`)。入力は `xdotool type`(エディタに `.focus()` したあと)。画面は `import -window root` で撮って自分で見る。

確認すること:
1. `$riff={c d e f} $riff $riff` で、構文エラーが出ず、「再生時間 4.0 秒」・音 8/3000・トラック 1/4。`$riff=` と `$riff` が command の色、定義の `{` `}` が構造の色になる。
2. `$m={c d} $m ; $m`: トラック 2/4・音 4。`$m={c d} ; $m`(定義だけの区間): トラック **1/4**(使用量の「トラック」の数え方が Rust と一致する)。
3. 位置つきのエラー(保存・試聴が `disabled`、「N 文字目」を押すとカーソルが移る): `$a={c} $ab`(8 文字目、「空白で区切ってください」)、`c $zzz`(3 文字目、未定義)、`$a={$a} $a`(5 文字目、自分自身)、`$a={o9} $a`(6 文字目、定義の中の `9`)、`$a={c;d} $a`(6 文字目)。
4. `$a={c} $a d` が構文エラーなし(`$a` と `d` が空白で区切られる)で、`$a={c} $ad` が未定義のエラーになる。
5. 「記法」を開くと、`$riff={c d e f} マクロ(…)` の行が出る(`$` と波括弧が文字で出ている)。
6. 手順 1 の MML で「試聴」を押し、`play_notify_sound` に `data:audio/wav;base64,…` が渡る(bridge で `fetch` の `ipc://` を包んで記録する)。**音が鳴ること自体は、Xvfb に音声デバイスが無いので確認できない**。そう書く。

確認が終わったら、自分で起動した Xvfb / `dbus-run-session` / アプリ / bridge のプロセスを、トークンで特定した PID を指定して止める(`pkill` / `killall` は使わない)。`/tmp` に作った隔離ホームと、Xvfb のロックファイルも片付ける。ユーザー自身の `cargo tauri dev` のプロセスには触れない。

- [ ] **Step 3: 結果を spec の末尾に記録する**

`docs/superpowers/specs/2026-10-20-issue-499-mml-macro-design.md` の末尾に、次の節を追記する(実際に確認したことと、確認できなかったことを、事実のとおりに書く。数値と所要時間はこの手順で得た実測値を入れる。括弧書きのまま残さない):

```markdown
## 実装後の確認

(確認した日付・ブランチ・HEAD のコミット、隔離インスタンスの起動方法を書く。)

確認できたこと:
- 自動テスト: `cargo test` の結果、`pnpm check` の結果、`pnpm test` の結果(件数)。
- `rustfmt --check src/mml.rs` の差分の数(`main` と同じ 5 かどうか)。
- 展開の上限と 300 段の連鎖(`macro_expansion_is_capped_and_deep_chains_work`)の所要時間。
- 実アプリ(隔離インスタンス): Step 2 の 1〜6 のそれぞれで、何を見て、何が起きたか。

確認できなかったこと:
- 実際に音が鳴ること(Xvfb に音声デバイスが無い)。
- 入力補助キー `$` の、モバイル UI での実際の押下。
- Android の実機・エミュレーター、macOS / Windows。
```

- [ ] **Step 4: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add docs/superpowers/specs/2026-10-20-issue-499-mml-macro-design.md && git commit -m "docs: MML のマクロの実装後の確認を記録(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: ブランチの扱いを決める(ユーザーに確認する)**

`superpowers:finishing-a-development-branch` に従う。PR を作る場合は、`.github/pull_request_template.md` の構成に手で合わせ、本文に `Refs #499` と書く(`Fixes` / `Closes` は書かない。#499 の候補が未着手で残るため)。`gh pr merge` は `--merge`(squash にしない。必須チェックの `e2e` が実行中だとブロックされるので、その場合は待たずにユーザーに伝える)。push 後に CI を待つループは作らない(ユーザーが見る)。#499 のチェックボックス(「変数・マクロ」)を更新するかは、ユーザーに確認してから行う。作業ブランチの削除も、ユーザーが頼んだときだけ行う。
