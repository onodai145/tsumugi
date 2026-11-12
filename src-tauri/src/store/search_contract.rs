//! 3 backend(SQLite / PostgreSQL / MySQL)の `search_cache` が同じ結果を返すことを固定する共有ケース
//! (Issue #583)。SQLite は通常のテストから、PostgreSQL / MySQL は `#[ignore]` の実 DB テストから呼ぶ。
//! 同じケースを、ライブ受信のインメモリ評価(`filter/eval.rs`)にも流して、SQL と一致することを確かめる。

use crate::domain::Note;
use crate::filter::ast::{CompareOp, Expr, Field, Value};
use crate::filter::eval::{evaluate, EvalContext};
use crate::filter::sql::{build_where, SqlCtx, SqlWhere};
use crate::mcp::testutil::public_note;
use crate::store::note_cache::NoteCacheBackend;

const ALL: &[&str] = &["t01", "t02", "t03", "t04", "t05", "t06", "t07", "t08", "t09", "t10", "t11", "t12", "t13", "t14", "t15"];

fn fixtures() -> Vec<Note> {
    let mut notes = vec![
        public_note("t01", 100, "100% sure"),
        public_note("t02", 101, "100 percent"),
        public_note("t03", 102, "a_b"),
        public_note("t04", 103, "aXb"),
        public_note("t05", 104, "Hello World"),
        public_note("t06", 105, "hello world"),
        public_note("t07", 106, "path\\to"),
        public_note("t08", 107, "wow!"),
        public_note("t09", 108, "日本語のテキスト😀"),
        public_note("t10", 109, "placeholder"),
        public_note("t11", 110, "x"),
        public_note("t12", 111, "Äpfel"),  // 非 ASCII の大文字(MySQL の _ai_ci は `Ä` と `ä` を同一視する)
        public_note("t13", 112, "äpfel"),
        public_note("t14", 113, "trail "), // 末尾空白あり
        public_note("t15", 114, "trail"),  // 末尾空白なし(PAD SPACE 照合順序だと t14 と等しくなる)
    ];
    notes[9].text = None; // t10: 本文なし。空文字列として扱われる。
    notes[10].cw = Some("Spoiler".into()); // t11: CW あり。他のノートは CW なし(空文字列として扱われる)。
    notes
}

struct Case {
    field: Field,
    op: CompareOp,
    needle: &'static str,
    expected: &'static [&'static str],
}

fn c(field: Field, op: CompareOp, needle: &'static str, expected: &'static [&'static str]) -> Case {
    Case { field, op, needle, expected }
}

fn cases() -> Vec<Case> {
    use CompareOp::{Contains, EndsWith, StartsWith};
    use Field::{CwText, Text};
    vec![
        // --- Contains: `%` `_` `\` `!` は実文字。大文字小文字を区別する。
        c(Text, Contains, "100%", &["t01"]),
        c(Text, Contains, "%", &["t01"]),
        c(Text, Contains, "_", &["t03"]),
        c(Text, Contains, "a_b", &["t03"]),
        c(Text, Contains, "\\", &["t07"]),
        c(Text, Contains, "!", &["t08"]),
        c(Text, Contains, "Hello", &["t05"]),
        c(Text, Contains, "hello", &["t06"]),
        c(Text, Contains, "HELLO", &[]),
        c(Text, Contains, "日本語", &["t09"]),
        c(Text, Contains, "😀", &["t09"]),
        c(Text, Contains, "", ALL),
        c(Text, Contains, "100% sure and more", &[]),
        // 非 ASCII の大文字小文字も区別する。
        c(Text, Contains, "Ä", &["t12"]),
        c(Text, Contains, "ä", &["t13"]),
        // 末尾空白は意味を持つ(PAD SPACE 照合順序でも、`trail ` と `trail` は別)。
        c(Text, Contains, "trail ", &["t14"]),
        c(Text, Contains, "trail  ", &[]),
        // --- StartsWith
        c(Text, StartsWith, "100", &["t01", "t02"]),
        c(Text, StartsWith, "100%", &["t01"]),
        c(Text, StartsWith, "a_", &["t03"]),
        c(Text, StartsWith, "Hello", &["t05"]),
        c(Text, StartsWith, "hello", &["t06"]),
        c(Text, StartsWith, "path\\", &["t07"]),
        c(Text, StartsWith, "日本", &["t09"]),
        c(Text, StartsWith, "", ALL),
        c(Text, StartsWith, "100% sure and more", &[]),
        c(Text, StartsWith, "Ä", &["t12"]),
        c(Text, StartsWith, "ä", &["t13"]),
        c(Text, StartsWith, "trail", &["t14", "t15"]),
        c(Text, StartsWith, "trail ", &["t14"]),
        // --- EndsWith
        c(Text, EndsWith, "sure", &["t01"]),
        c(Text, EndsWith, "percent", &["t02"]),
        c(Text, EndsWith, "to", &["t07"]),
        c(Text, EndsWith, "b", &["t03", "t04"]),
        c(Text, EndsWith, "_b", &["t03"]),
        c(Text, EndsWith, "!", &["t08"]),
        c(Text, EndsWith, "😀", &["t09"]),
        c(Text, EndsWith, "World", &["t05"]),
        c(Text, EndsWith, "", ALL),
        c(Text, EndsWith, "100% sure and more", &[]),
        c(Text, EndsWith, "pfel", &["t12", "t13"]),
        c(Text, EndsWith, "trail", &["t15"]),
        c(Text, EndsWith, "trail ", &["t14"]),
        // --- NULL の cw / text は空文字列として扱われる(空の検索語にだけ一致)。
        c(CwText, Contains, "Spoil", &["t11"]),
        c(CwText, Contains, "spoil", &[]),
        c(CwText, Contains, "", ALL),
        c(CwText, StartsWith, "Spoil", &["t11"]),
        c(CwText, EndsWith, "ler", &["t11"]),
        c(Text, Contains, "placeholder", &[]),
    ]
}

/// 部分一致(`->` / `startswith` / `endswith`)が、3 backend で同じ結果になること。
pub(crate) async fn text_match_contract(b: &dyn NoteCacheBackend) {
    run_cases(b, cases()).await;
}

/// 正規表現(`match`、Issue #598)が、3 backend とインメモリ評価で同じ結果になること。
/// 方言をまたいで同じ意味になる構文(`^` `$` `.` `[0-9]` `|`)だけを使う。大文字小文字を区別する。
pub(crate) async fn regex_contract(b: &dyn NoteCacheBackend) {
    run_cases(b, regex_cases()).await;
}

fn regex_cases() -> Vec<Case> {
    use CompareOp::Match;
    use Field::{CwText, Text};
    vec![
        // 大文字小文字を区別する(MySQL の既定照合順序 `_ci` に引きずられない)。
        c(Text, Match, "Hello", &["t05"]),
        c(Text, Match, "hello", &["t06"]),
        c(Text, Match, "HELLO", &[]),
        c(Text, Match, "^Hello", &["t05"]),
        c(Text, Match, "World$", &["t05"]),
        c(Text, Match, "Hello|hello", &["t05", "t06"]),
        // 非 ASCII の大文字小文字も区別する。
        c(Text, Match, "Ä", &["t12"]),
        c(Text, Match, "ä", &["t13"]),
        c(Text, Match, "日本語", &["t09"]),
        c(Text, Match, "テキスト😀$", &["t09"]),
        // メタ文字。
        c(Text, Match, "a.b", &["t03", "t04"]),
        c(Text, Match, "^[0-9]+", &["t01", "t02"]),
        c(Text, Match, "wow!$", &["t08"]),
        // 末尾空白は意味を持つ。
        c(Text, Match, "^trail $", &["t14"]),
        c(Text, Match, "^trail$", &["t15"]),
        // NULL の text / cw は空文字列として扱われる。
        c(Text, Match, "^$", &["t10"]),
        c(CwText, Match, "^Spoiler$", &["t11"]),
        c(CwText, Match, "^spoiler$", &[]),
    ]
}

async fn run_cases(b: &dyn NoteCacheBackend, cases: Vec<Case>) {
    let notes = fixtures();
    b.cache_notes("col1", &notes).await.unwrap();
    let ctx = SqlCtx { my_ids: vec![], following_ids: None };
    let eval_ctx = EvalContext::default();

    for case in cases {
        let expr = Expr::Compare {
            lhs: Value::Field(case.field),
            op: case.op,
            rhs: Value::Str(case.needle.to_string()),
        };
        let want: Vec<String> = case.expected.iter().map(|s| s.to_string()).collect();
        let label = format!("{:?} {:?} {:?}", case.field, case.op, case.needle);

        let w = build_where(&expr, &ctx).unwrap();
        let mut got: Vec<String> = b.search_cache(&w, None, 100).await.unwrap().into_iter().map(|n| n.id).collect();
        got.sort();
        assert_eq!(got, want, "SQL: {label}");

        // ライブ受信のインメモリ評価とも一致する。
        let mut mem: Vec<String> = notes.iter().filter(|n| evaluate(&expr, n, &eval_ctx)).map(|n| n.id.clone()).collect();
        mem.sort();
        assert_eq!(mem, want, "eval: {label}");
    }
}

/// ページ境界が ID 基準で、`created_at` と ID の順序が食い違うノートでも欠落も重複も起きないこと。
pub(crate) async fn paging_contract(b: &dyn NoteCacheBackend) {
    // ID の昇順: a0 < a1 < a2 < a3 < z9。`created_at` の新しい順: a0(999) > a3 > a2 > a1 > z9(10)。
    let notes = vec![
        public_note("a0", 999, "newest by time, smallest id"),
        public_note("a1", 100, "x"),
        public_note("a2", 200, "x"),
        public_note("a3", 300, "x"),
        public_note("z9", 10, "oldest by time, largest id"),
    ];
    b.cache_notes("col1", &notes).await.unwrap();
    let w = SqlWhere::always_true();
    let ids = |notes: Vec<Note>| -> Vec<String> { notes.into_iter().map(|n| n.id).collect() };

    // 先頭ページは ID の大きい順。
    assert_eq!(ids(b.search_cache(&w, None, 2).await.unwrap()), ["z9", "a3"]);

    // 各ページの最後の ID をカーソルにしてたどると、全ノートが1回ずつ現れる。
    let mut seen: Vec<String> = vec![];
    let mut cursor: Option<String> = None;
    for _ in 0..10 {
        let page = b.search_cache(&w, cursor.as_deref(), 2).await.unwrap();
        let Some(last) = page.last() else { break };
        cursor = Some(last.id.clone());
        seen.extend(ids(page));
    }
    assert_eq!(seen, ["z9", "a3", "a2", "a1", "a0"]);

    // 存在しない ID をカーソルにしても、その ID より小さいものが続く("a2" < "a25" < "a3")。
    assert_eq!(ids(b.search_cache(&w, Some("a25"), 10).await.unwrap()), ["a2", "a1", "a0"]);
}
