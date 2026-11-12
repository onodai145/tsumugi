//! 3 backend(SQLite / PostgreSQL / MySQL)の `search_cache` が同じ結果を返すことを固定する共有ケース
//! (Issue #583)。SQLite は通常のテストから、PostgreSQL / MySQL は `#[ignore]` の実 DB テストから呼ぶ。
//! 同じケースを、ライブ受信のインメモリ評価(`filter/eval.rs`)にも流して、SQL と一致することを確かめる。

use crate::domain::Note;
use crate::filter::ast::{CompareOp, Expr, Field, Value};
use crate::filter::eval::{evaluate, EvalContext};
use crate::filter::sql::{build_where, SqlCtx};
use crate::mcp::testutil::public_note;
use crate::store::note_cache::NoteCacheBackend;

const ALL: &[&str] = &["t01", "t02", "t03", "t04", "t05", "t06", "t07", "t08", "t09", "t10", "t11"];

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
    let notes = fixtures();
    b.cache_notes("col1", &notes).await.unwrap();
    let ctx = SqlCtx { my_ids: vec![], following_ids: None };
    let eval_ctx = EvalContext::default();

    for case in cases() {
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
