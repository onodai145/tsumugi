# TQL入力補完 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `AddColumnModal` のTQL入力欄（エキスパートモードのtextarea・簡単モードのfilter input）に、文脈に応じた入力補完ドロップダウンを追加する。

**Architecture:** Rust側に新設する `filter/complete.rs` がカーソル位置までのテキストを文脈分類し（ソース名/フィールド名/演算子/キーワード）、新規コマンド `tql_complete` で公開する。`list`/`antenna`/`channel` の引数(実ID)候補はフロントが既に保持するアカウントのリスト/アンテナ/チャンネル一覧から直接生成する。フロントは既存の`CompletionPopover.svelte`（本文MFM補完で使用中の汎用ポップアップ）をそのまま再利用し、新設する`TqlCompletionField.svelte`が入力欄・トリガー検出・候補取得・キーボード操作をまとめて提供する。

**Tech Stack:** Rust(Tauri v2, tauri-specta), Svelte 5(runes), TypeScript, Vitest。

## Global Constraints

- 補完対象はAddColumnModalの2箇所のみ: エキスパートtextarea（`Query`モード）・簡単モードfilter input（`Predicate`モード）。
- `tql_complete`はエラーを返さない（`Vec::new()`で握りつぶす。バリデーション表示とは独立）。
- `list`/`antenna`/`channel`のID引数候補はRustを呼ばずフロント側の既存データ（`lists`/`antennas`/`channels` state）から生成する。
- 新規コマンドは `specta_builder()` に登録し、TSバインディングを regenerate すること。
- `docs/design/filter-dsl-design.md` §10 のフィールド表がフィールド名候補の正（canonical表記のみ、エイリアスは候補に出さない）。

---

### Task 1: Rust補完エンジン (`filter/complete.rs`)

**Files:**
- Create: `src-tauri/src/filter/complete.rs`
- Modify: `src-tauri/src/filter/mod.rs`

**Interfaces:**
- Produces: `pub enum TqlEditMode { Query, Predicate }`、`pub enum TqlCompletionKind { Keyword, Source, Field, Operator }`、`pub struct TqlCompletionItem { pub label: String, pub insert: String, pub kind: TqlCompletionKind }`、`pub fn complete(text: &str, cursor_chars: usize, mode: TqlEditMode) -> Vec<TqlCompletionItem>`（Task 2が使う）。
- Consumes: `super::ast::Field::from_name`、`super::token::{tokenize, Token}`（既存）。

- [ ] **Step 1: `filter/complete.rs` を作成し、型・定数・`complete()`本体を実装する**

```rust
//! TQL入力補完。docs/design/filter-dsl-design.md の文法に基づき、カーソル位置までの
//! 部分入力を文脈分類し、キーワード/ソース名/フィールド名/演算子の候補を返す。
//! list/antenna/channel の引数(実ID)はここでは扱わない(フロント側で別途解決、
//! docs/superpowers/specs/2026-08-05-tql-autocomplete-design.md §2)。

use super::ast::Field;
use super::token::{tokenize, Token};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TqlEditMode {
    /// from ... where ... のフルクエリ(エキスパートモードのtextarea)
    Query,
    /// where 述語のみ(簡単モードのfilter input)
    Predicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TqlCompletionKind {
    Keyword,
    Source,
    Field,
    Operator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TqlCompletionItem {
    pub label: String,
    pub insert: String,
    pub kind: TqlCompletionKind,
}

const ARGLESS_SOURCES: &[&str] = &["home", "local", "hybrid", "global", "mentions", "cache"];
const ARG_SOURCES: &[&str] = &["list", "antenna", "channel", "user", "tag", "search"];

const FIELD_NAMES: &[&str] = &[
    "renote", "quote", "reply", "has_files", "has_poll", "cw", "sensitive", "local", "remote",
    "bot", "cat", "direct", "to_me", "reply_to_me", "has_mention", "has_link", "pinned",
    "reacted", "renoted", "favorited", "mine", "following", "reactions", "renotes", "replies",
    "files", "length", "created_at", "user.followers", "user.following", "user.notes", "text",
    "cw_text", "via", "host", "visibility", "channel", "lang", "reply_id", "renote_id",
    "user.username", "user.acct", "user.name", "user.id", "tags", "mentions", "emojis",
    "file_types",
];

const WORD_OPERATORS: &[&str] = &["contains", "in", "startswith", "endswith", "match"];
const SYMBOL_OPERATORS: &[&str] = &["==", "!=", "<", ">", "<=", ">=", "->", "<-"];
const LOGIC_OPERATORS: &[&str] = &["&&", "||"];

/// `text` の先頭から `cursor_chars`（Unicodeコードポイント数。フロントがJSのUTF-16
/// カーソル位置から変換して渡す）文字目までを文脈分類し、前方一致する補完候補を返す。
/// tokenize失敗時は空配列を返す(入力途中の不正な文字列でも落とさない)。
pub fn complete(text: &str, cursor_chars: usize, mode: TqlEditMode) -> Vec<TqlCompletionItem> {
    let cursor = char_offset_to_byte(text, cursor_chars);
    let (word_start, partial) = current_word(text, cursor);
    let prefix = &text[..word_start];
    let Ok(tokens) = tokenize(prefix) else {
        return Vec::new();
    };

    let partial_lower = partial.to_lowercase();
    classify(&tokens, mode)
        .into_iter()
        .filter(|item| item.label.to_lowercase().starts_with(&partial_lower))
        .collect()
}

fn char_offset_to_byte(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// カーソル直前の識別子文字列(ASCII英数字と'_')を後方スキャンで切り出す。
/// ASCIIバイトだけを見て止まるため、常にUTF-8の文字境界上で止まる
/// (マルチバイト文字のバイトは is_ascii_alphanumeric() が常にfalseなので跨がない)。
fn current_word(text: &str, cursor: usize) -> (usize, String) {
    let bytes = text.as_bytes();
    let mut start = cursor;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }
    (start, text[start..cursor].to_string())
}

fn classify(tokens: &[Token], mode: TqlEditMode) -> Vec<TqlCompletionItem> {
    if mode == TqlEditMode::Predicate {
        return predicate_context(tokens);
    }
    let has_from = tokens.iter().any(|t| matches!(t, Token::Ident(s) if s == "from"));
    if !has_from {
        return if tokens.is_empty() {
            vec![keyword_item("from")]
        } else {
            Vec::new()
        };
    }
    let has_where = tokens.iter().any(|t| matches!(t, Token::Ident(s) if s == "where"));
    if !has_where {
        return source_context(tokens);
    }
    predicate_context(tokens)
}

fn source_context(tokens: &[Token]) -> Vec<TqlCompletionItem> {
    match tokens.last() {
        Some(Token::Ident(s)) if s == "from" => source_items(),
        Some(Token::Comma) => source_items(),
        Some(Token::Ident(s)) if ARGLESS_SOURCES.contains(&s.as_str()) => {
            let mut items = source_items();
            items.push(keyword_item("where"));
            items
        }
        Some(Token::RParen) => {
            let mut items = source_items();
            items.push(keyword_item("where"));
            items
        }
        _ => Vec::new(),
    }
}

fn predicate_context(tokens: &[Token]) -> Vec<TqlCompletionItem> {
    match tokens.last() {
        None => field_items(),
        Some(Token::AndAnd) | Some(Token::OrOr) | Some(Token::Not) | Some(Token::LParen) => {
            field_items()
        }
        Some(Token::Ident(s)) if s == "where" => field_items(),
        Some(Token::Ident(s)) if Field::from_name(s).is_some() => operator_items(),
        Some(Token::Str(_)) | Some(Token::Num(_)) | Some(Token::RBracket) | Some(Token::RParen) => {
            logic_items()
        }
        _ => Vec::new(),
    }
}

fn keyword_item(name: &str) -> TqlCompletionItem {
    TqlCompletionItem {
        label: name.to_string(),
        insert: format!("{name} "),
        kind: TqlCompletionKind::Keyword,
    }
}

fn source_items() -> Vec<TqlCompletionItem> {
    let mut items: Vec<TqlCompletionItem> = ARGLESS_SOURCES
        .iter()
        .map(|s| TqlCompletionItem {
            label: s.to_string(),
            insert: format!("{s} "),
            kind: TqlCompletionKind::Source,
        })
        .collect();
    items.extend(ARG_SOURCES.iter().map(|s| TqlCompletionItem {
        label: s.to_string(),
        insert: format!("{s}(\""),
        kind: TqlCompletionKind::Source,
    }));
    items
}

fn field_items() -> Vec<TqlCompletionItem> {
    FIELD_NAMES
        .iter()
        .map(|f| TqlCompletionItem {
            label: f.to_string(),
            insert: format!("{f} "),
            kind: TqlCompletionKind::Field,
        })
        .collect()
}

fn operator_items() -> Vec<TqlCompletionItem> {
    WORD_OPERATORS
        .iter()
        .chain(SYMBOL_OPERATORS.iter())
        .chain(LOGIC_OPERATORS.iter())
        .map(|op| TqlCompletionItem {
            label: op.to_string(),
            insert: format!("{op} "),
            kind: TqlCompletionKind::Operator,
        })
        .collect()
}

fn logic_items() -> Vec<TqlCompletionItem> {
    LOGIC_OPERATORS
        .iter()
        .map(|op| TqlCompletionItem {
            label: op.to_string(),
            insert: format!("{op} "),
            kind: TqlCompletionKind::Operator,
        })
        .collect()
}
```

- [ ] **Step 2: 同じファイルの末尾にユニットテストを追加する**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[TqlCompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn suggests_from_at_empty_query() {
        let items = complete("", 0, TqlEditMode::Query);
        assert_eq!(labels(&items), vec!["from"]);
    }

    #[test]
    fn suggests_sources_after_from() {
        let items = complete("from ", 5, TqlEditMode::Query);
        assert!(labels(&items).contains(&"home"));
        assert!(labels(&items).contains(&"list"));
        assert!(!labels(&items).contains(&"where"));
    }

    #[test]
    fn suggests_where_after_a_bare_source() {
        let items = complete("from home ", 10, TqlEditMode::Query);
        assert!(labels(&items).contains(&"where"));
        assert!(labels(&items).contains(&"local"));
    }

    #[test]
    fn suggests_sources_after_comma() {
        let items = complete("from home, ", 11, TqlEditMode::Query);
        assert!(labels(&items).contains(&"list"));
        assert!(!labels(&items).contains(&"where"));
    }

    #[test]
    fn suggests_fields_after_where() {
        let items = complete("from home where ", 16, TqlEditMode::Query);
        assert!(labels(&items).contains(&"has_files"));
        assert!(labels(&items).contains(&"reactions"));
    }

    #[test]
    fn filters_fields_by_partial_prefix() {
        let items = complete("from home where has_fi", 22, TqlEditMode::Query);
        assert_eq!(labels(&items), vec!["has_files"]);
    }

    #[test]
    fn suggests_operators_after_a_field_name() {
        let items = complete("from home where reactions ", 26, TqlEditMode::Query);
        assert!(labels(&items).contains(&"contains"));
        assert!(labels(&items).contains(&">="));
        assert!(labels(&items).contains(&"&&"));
    }

    #[test]
    fn suggests_logic_operators_after_a_value() {
        let items = complete("from home where reactions >= 10 ", 32, TqlEditMode::Query);
        assert_eq!(labels(&items), vec!["&&", "||"]);
    }

    #[test]
    fn predicate_mode_suggests_fields_from_the_start() {
        let items = complete("has_fi", 6, TqlEditMode::Predicate);
        assert_eq!(labels(&items), vec!["has_files"]);
    }

    #[test]
    fn predicate_mode_suggests_fields_after_and_and() {
        let items = complete("has_files && ", 13, TqlEditMode::Predicate);
        assert!(labels(&items).contains(&"cw"));
    }

    #[test]
    fn returns_empty_on_broken_input() {
        let items = complete("from list(\"", 11, TqlEditMode::Query);
        assert!(items.is_empty());
    }

    #[test]
    fn handles_multibyte_content_before_cursor_safely() {
        // 日本語文字列リテラルなど、cursor手前にマルチバイト文字が含まれていても
        // バイトオフセット変換でパニックせず文脈分類が正しく続くことを確認する。
        let items = complete("text -> \"日本語\" && has_fi", 23, TqlEditMode::Predicate);
        assert_eq!(labels(&items), vec!["has_files"]);
    }
}
```

- [ ] **Step 3: `filter/mod.rs` に `pub mod complete;` を追加する**

`src-tauri/src/filter/mod.rs` の既存の `pub mod` 群（`ast, eval, mute, parser, sql, token`）にアルファベット順で挿入:

```rust
pub mod ast;
pub mod complete;
pub mod eval;
pub mod mute;
pub mod parser;
pub mod sql;
pub mod token;
```

- [ ] **Step 4: テストを実行して全て通ることを確認する**

Run: `cd src-tauri && cargo test filter::complete`
Expected: 11個のテストが全て `PASS`

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/filter/complete.rs src-tauri/src/filter/mod.rs
git commit -m "feat: TQL補完エンジンを追加"
```

---

### Task 2: `tql_complete` コマンドの公開とTSバインディング再生成

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Generated: `frontend/src/bindings/tauri.gen.ts`（`cargo test` で自動再生成、手編集しない）

**Interfaces:**
- Consumes: Task 1の `crate::filter::complete::{complete, TqlEditMode, TqlCompletionItem}`。
- Produces: フロントから呼べる `tql_complete(text, cursor, mode)` コマンド、およびTS側の `TqlEditMode` / `TqlCompletionItem` 型（Task 4・5が使う）。

- [ ] **Step 1: `commands/column.rs` に `validate_tql_query` の直後へコマンドを追加する**

`src-tauri/src/commands/column.rs:477`（`validate_tql_query` 関数の閉じ括弧の直後）に挿入:

```rust

/// TQL入力補完。カーソル位置までの部分入力を文脈分類し、候補一覧を返す。
/// list/antenna/channel の実ID候補はフロント側で別途解決する(このコマンドは構文語彙のみ)。
#[tauri::command]
#[specta::specta]
pub fn tql_complete(
    text: String,
    cursor: u32,
    mode: crate::filter::complete::TqlEditMode,
) -> Vec<crate::filter::complete::TqlCompletionItem> {
    crate::filter::complete::complete(&text, cursor as usize, mode)
}
```

- [ ] **Step 2: `commands/mod.rs` の再エクスポート一覧に追加する**

`src-tauri/src/commands/mod.rs:17-23` の `pub use column::{ ... }` 内、`set_group_auto, set_group_width,` の並びの直後（アルファベット順）に `tql_complete,` を挿入:

```rust
pub use column::{
    add_column, capture_notes, close_column, fetch_backfill, fetch_notifications_backfill,
    list_antennas, list_channels, list_columns, list_groups, list_user_lists, move_tab,
    note_count, notes_since, rename_column, reorder_groups, resolve_user_acct, resume_column,
    set_group_auto, set_group_width, tql_complete, uncapture_notes, update_column, validate_filter,
    validate_tql_query, OpenedColumn,
};
```

- [ ] **Step 3: `lib.rs` の `specta_builder()` に登録する**

`src-tauri/src/lib.rs` の `commands::column::validate_tql_query,` の直後に追加:

```rust
            commands::column::validate_tql_query,
            commands::column::tql_complete,
```

- [ ] **Step 4: TSバインディングを再生成する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: `PASS`。`frontend/src/bindings/tauri.gen.ts` に `tqlComplete`・`TqlEditMode`・`TqlCompletionItem` が追記されていることを確認する:

Run: `grep -n "tqlComplete\|TqlEditMode\|TqlCompletionItem" frontend/src/bindings/tauri.gen.ts`
Expected: 3つとも出力に含まれる

- [ ] **Step 5: Rust全体のビルド・テストを確認する**

Run: `cd src-tauri && cargo test`
Expected: 既存テストも含めて全て `PASS`

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: tql_completeコマンドを追加しTSバインディングを再生成"
```

---

### Task 3: フロント側の補完ロジック (`lib/tqlCompletion.ts`)

**Files:**
- Create: `frontend/src/lib/tqlCompletion.ts`
- Test: `frontend/src/lib/tqlCompletion.test.ts`

**Interfaces:**
- Consumes: `TqlCompletionItem`（Task 2で生成される `../bindings/tauri.gen`）、`CompletionItem`（既存 `./mfmCompletion` の型 `{ key: string; label: string; insertText: string; thumbnail?: ... }`）、`UserList` / `SourceItem`（既存 `../bindings/tauri.gen`、どちらも `{ id: string; name: string }`）。
- Produces: `TqlTrigger { start: number; end: number }`、`detectIdArgTrigger(text, cursor)`、`currentWordTrigger(text, cursor)`、`charOffset(text, cursor)`、`idCandidates(kind, query, lists, antennas, channels)`、`syntaxCandidates(items)`、`applyTqlCompletion(text, trigger, item)`（Task 5が使う）。

- [ ] **Step 1: 失敗するテストを先に書く**

`frontend/src/lib/tqlCompletion.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  applyTqlCompletion,
  charOffset,
  currentWordTrigger,
  detectIdArgTrigger,
  idCandidates,
  syntaxCandidates,
} from "./tqlCompletion";

describe("detectIdArgTrigger", () => {
  it("detects an unterminated list( argument string", () => {
    expect(detectIdArgTrigger('from list("ab', 13)).toEqual({
      trigger: { start: 11, end: 13 },
      kind: "list",
      query: "ab",
    });
  });

  it("detects an unterminated antenna( argument string with an empty query", () => {
    expect(detectIdArgTrigger('from antenna("', 14)).toEqual({
      trigger: { start: 14, end: 14 },
      kind: "antenna",
      query: "",
    });
  });

  it("returns null once the string literal is closed", () => {
    expect(detectIdArgTrigger('from list("ab")', 15)).toBeNull();
  });

  it("returns null for sources without id arguments", () => {
    expect(detectIdArgTrigger('from tag("ab', 12)).toBeNull();
  });
});

describe("currentWordTrigger", () => {
  it("captures the identifier being typed", () => {
    expect(currentWordTrigger("from home where has_fi", 22)).toEqual({ start: 16, end: 22 });
  });

  it("returns a zero-length span right after a space", () => {
    expect(currentWordTrigger("from home where ", 16)).toEqual({ start: 16, end: 16 });
  });
});

describe("charOffset", () => {
  it("counts unicode code points, not UTF-16 units", () => {
    // "😀" はUTF-16では2単位(サロゲートペア)だが、コードポイントは1
    expect(charOffset("😀abc", 5)).toBe(4);
  });
});

describe("idCandidates", () => {
  const lists = [
    { id: "l1", name: "Friends" },
    { id: "l2", name: "Work" },
  ];

  it("filters by prefix (case-insensitive) and inserts the id, closing the argument", () => {
    expect(idCandidates("list", "fr", lists, [], [])).toEqual([
      { key: "l1", label: "Friends", insertText: 'l1")' },
    ]);
  });
});

describe("syntaxCandidates", () => {
  it("maps Rust completion items to the CompletionItem shape", () => {
    expect(syntaxCandidates([{ label: "has_files", insert: "has_files ", kind: "field" }])).toEqual([
      { key: "has_files", label: "has_files", insertText: "has_files " },
    ]);
  });
});

describe("applyTqlCompletion", () => {
  it("replaces the trigger span and places the cursor at the end of the inserted text", () => {
    const result = applyTqlCompletion(
      "from home where has_fi",
      { start: 16, end: 22 },
      { key: "has_files", label: "has_files", insertText: "has_files " },
    );
    expect(result).toEqual({ text: "from home where has_files ", cursor: 26 });
  });
});
```

- [ ] **Step 2: テストを実行し、モジュール未実装で失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/tqlCompletion.test.ts`
Expected: FAIL（`Cannot find module './tqlCompletion'` 等）

- [ ] **Step 3: `lib/tqlCompletion.ts` を実装する**

```ts
import type { SourceItem, TqlCompletionItem, UserList } from "../bindings/tauri.gen";
import type { CompletionItem } from "./mfmCompletion";

export interface TqlTrigger {
  start: number;
  end: number;
}

export interface TqlIdTrigger {
  trigger: TqlTrigger;
  kind: "list" | "antenna" | "channel";
  query: string;
}

// list("..." / antenna("..." / channel("..." の、閉じられていない文字列リテラルの中を検出する。
// (閉じ引用符が既にある場合はマッチしない = tokenize可能な通常の構文補完へフォールバックする)
const ID_ARG_RE = /(list|antenna|channel)\(\s*"([^"]*)$/;
const WORD_CHAR = /[A-Za-z0-9_]/;

export function detectIdArgTrigger(text: string, cursor: number): TqlIdTrigger | null {
  const head = text.slice(0, cursor);
  const m = ID_ARG_RE.exec(head);
  if (!m) return null;
  const [, kind, query] = m;
  return {
    trigger: { start: cursor - query.length, end: cursor },
    kind: kind as "list" | "antenna" | "channel",
    query,
  };
}

// カーソル直前の識別子(ASCII英数字と'_')の範囲を返す。Rust側 current_word() と同じ規則。
export function currentWordTrigger(text: string, cursor: number): TqlTrigger {
  let start = cursor;
  while (start > 0 && WORD_CHAR.test(text[start - 1])) start--;
  return { start, end: cursor };
}

// JSのカーソル位置(UTF-16コード単位)をUnicodeコードポイント数へ変換する
// (Rust側は cursor_chars をコードポイント数として受け取るため)。
export function charOffset(text: string, cursor: number): number {
  return [...text.slice(0, cursor)].length;
}

function startsWithCI(s: string, query: string): boolean {
  return s.toLowerCase().startsWith(query.toLowerCase());
}

export function idCandidates(
  kind: "list" | "antenna" | "channel",
  query: string,
  lists: UserList[],
  antennas: SourceItem[],
  channels: SourceItem[],
): CompletionItem[] {
  const pool = kind === "list" ? lists : kind === "antenna" ? antennas : channels;
  return pool
    .filter((x) => startsWithCI(x.name || x.id, query))
    .map((x) => ({ key: x.id, label: x.name || x.id, insertText: `${x.id}")` }));
}

export function syntaxCandidates(items: TqlCompletionItem[]): CompletionItem[] {
  return items.map((it) => ({ key: it.label, label: it.label, insertText: it.insert }));
}

export function applyTqlCompletion(
  text: string,
  trigger: TqlTrigger,
  item: CompletionItem,
): { text: string; cursor: number } {
  const next = text.slice(0, trigger.start) + item.insertText + text.slice(trigger.end);
  return { text: next, cursor: trigger.start + item.insertText.length };
}
```

- [ ] **Step 4: テストを実行して通ることを確認する**

Run: `cd frontend && pnpm vitest run src/lib/tqlCompletion.test.ts`
Expected: 全テスト `PASS`

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/tqlCompletion.ts frontend/src/lib/tqlCompletion.test.ts
git commit -m "feat: TQL補完のフロント側ロジックを追加"
```

---

### Task 4: `app.tqlComplete` ラッパー

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.tqlComplete`（Task 2で生成、`./ipc` 経由）。
- Produces: `app.tqlComplete(text, cursor, mode): Promise<TqlCompletionItem[]>`（Task 5が使う）。

- [ ] **Step 1: 型インポートに `TqlEditMode`, `TqlCompletionItem` を追加する**

`frontend/src/lib/store.svelte.ts:12-31` の `import type { ... } from "../bindings/tauri.gen";` ブロック内、`PaneNode,` の直後に追加:

```ts
  PaneNode,
  TqlEditMode,
  TqlCompletionItem,
} from "../bindings/tauri.gen";
```

- [ ] **Step 2: `validateTqlQuery` の直後にラッパーメソッドを追加する**

`frontend/src/lib/store.svelte.ts:1042-1045`（`validateTqlQuery` メソッドの閉じ括弧の直後）に挿入:

```ts

  /// TQL入力補完。エラーは返さない(Rust側でVec::new()に握りつぶし済み)。
  async tqlComplete(text: string, cursor: number, mode: TqlEditMode): Promise<TqlCompletionItem[]> {
    return commands.tqlComplete(text, cursor, mode);
  }
```

- [ ] **Step 3: 型チェックを通す**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 4: コミット**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: app.tqlCompleteラッパーを追加"
```

---

### Task 5: `TqlCompletionField.svelte` コンポーネント

**Files:**
- Create: `frontend/src/input/TqlCompletionField.svelte`

**Interfaces:**
- Consumes: Task 3の `lib/tqlCompletion.ts` 一式、Task 4の `app.tqlComplete`、既存の `ui/CompletionPopover.svelte`（`items: CompletionItem[]`, `selectedIndex: number`, `left/top: number`, `onpick: (index:number)=>void`）、既存の `lib/caretPosition.ts` の `getCaretCoordinates(el: HTMLTextAreaElement, position: number): {left, top, height}`。
- Produces: `<TqlCompletionField mode value placeholder rows invalid oninput lists antennas channels />`（Task 6がAddColumnModalから使う。`value` は `$bindable`）。

このコンポーネントは、既にComposeBar.svelteのMFM本文補完（`mfmCompletion.ts` + `caretPosition.ts` + `CompletionPopover.svelte`）で確立されている「`trigger`/`candidates`/`popoverOpen`をderivedで作り、`onKeydown`でArrow/Tab/Enter/Escapeを処理する」パターンをそのまま踏襲する。textarea/inputはこのコンポーネント自身が描画する(親はbind:valueで値を受け取るだけ)。

- [ ] **Step 1: コンポーネントを実装する**

```svelte
<script lang="ts">
  import { tick } from "svelte";
  import { app } from "../lib/store.svelte";
  import CompletionPopover from "../ui/CompletionPopover.svelte";
  import { getCaretCoordinates } from "../lib/caretPosition";
  import {
    applyTqlCompletion,
    charOffset,
    currentWordTrigger,
    detectIdArgTrigger,
    idCandidates,
    syntaxCandidates,
    type TqlTrigger,
  } from "../lib/tqlCompletion";
  import type { SourceItem, TqlCompletionItem, TqlEditMode, UserList } from "../bindings/tauri.gen";
  import type { CompletionItem } from "../lib/mfmCompletion";

  let {
    mode,
    value = $bindable(),
    placeholder = "",
    rows,
    invalid = false,
    oninput,
    lists = [],
    antennas = [],
    channels = [],
  }: {
    mode: TqlEditMode;
    value: string;
    placeholder?: string;
    rows?: number;
    invalid?: boolean;
    oninput?: () => void;
    lists?: UserList[];
    antennas?: SourceItem[];
    channels?: SourceItem[];
  } = $props();

  let el = $state<HTMLTextAreaElement | HTMLInputElement | undefined>(undefined);
  let cursorPos = $state(0);
  let suppressAt = $state<number | null>(null);
  let composing = $state(false);
  let selectedIndex = $state(0);
  let selectionMoved = $state(false);
  let rustItems = $state<TqlCompletionItem[]>([]);
  let fetchToken = 0;

  const idTrigger = $derived(mode === "query" ? detectIdArgTrigger(value, cursorPos) : null);

  const trigger = $derived<TqlTrigger | null>(
    composing || cursorPos === suppressAt ? null : (idTrigger?.trigger ?? currentWordTrigger(value, cursorPos)),
  );

  // ID引数の文脈(list("...")等)ではRustを呼ばない。それ以外は都度 tql_complete を呼ぶ
  // (IPCはローカル呼び出しでネットワークを介さないため、デバウンスはせず世代カウンタで
  // 古い応答だけ無視する)。
  $effect(() => {
    if (composing || cursorPos === suppressAt || idTrigger) {
      rustItems = [];
      return;
    }
    const text = value;
    const cursor = cursorPos;
    const token = ++fetchToken;
    app
      .tqlComplete(text, charOffset(text, cursor), mode)
      .then((items) => {
        if (token === fetchToken) rustItems = items;
      })
      .catch(() => {
        if (token === fetchToken) rustItems = [];
      });
  });

  const candidates = $derived<CompletionItem[]>(
    !trigger
      ? []
      : idTrigger
        ? idCandidates(idTrigger.kind, idTrigger.query, lists, antennas, channels)
        : syntaxCandidates(rustItems),
  );
  const popoverOpen = $derived(trigger !== null && candidates.length > 0);

  // クエリ(文脈)が変わるたびに選択位置を先頭へ戻す
  $effect(() => {
    trigger;
    selectedIndex = 0;
    selectionMoved = false;
  });

  let popoverPos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!popoverOpen || !trigger || !el) {
      popoverPos = null;
      return;
    }
    const rect = el.getBoundingClientRect();
    if (el instanceof HTMLTextAreaElement) {
      const caret = getCaretCoordinates(el, trigger.start);
      popoverPos = { left: rect.left + caret.left, top: rect.top + caret.top + caret.height };
    } else {
      popoverPos = { left: rect.left, top: rect.bottom + 4 };
    }
  });

  function syncCursor() {
    const pos = el?.selectionStart ?? 0;
    if (pos !== cursorPos) suppressAt = null;
    cursorPos = pos;
  }

  async function confirmCompletion(index: number) {
    const t = trigger;
    const item = candidates[index];
    if (!t || !item) return;
    const result = applyTqlCompletion(value, t, item);
    value = result.text;
    suppressAt = result.cursor;
    await tick();
    el?.setSelectionRange(result.cursor, result.cursor);
    el?.focus();
    cursorPos = result.cursor;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!popoverOpen) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIndex = selectionMoved ? Math.min(selectedIndex + 1, candidates.length - 1) : 0;
      selectionMoved = true;
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIndex = selectionMoved ? Math.max(selectedIndex - 1, 0) : candidates.length - 1;
      selectionMoved = true;
      return;
    }
    if (e.key === "Tab" || e.key === "Enter") {
      e.preventDefault();
      confirmCompletion(selectedIndex);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      suppressAt = cursorPos;
    }
  }

  function onInputHandler() {
    syncCursor();
    suppressAt = null;
    oninput?.();
  }
</script>

{#if mode === "query"}
  <textarea
    class:invalid
    {rows}
    {placeholder}
    bind:value
    bind:this={el}
    onkeydown={onKeydown}
    onkeyup={syncCursor}
    onclick={syncCursor}
    oninput={onInputHandler}
    oncompositionstart={() => (composing = true)}
    oncompositionend={() => {
      composing = false;
      syncCursor();
    }}
    onblur={() => (suppressAt = cursorPos)}
  ></textarea>
{:else}
  <input
    class:invalid
    {placeholder}
    bind:value
    bind:this={el}
    onkeydown={onKeydown}
    onkeyup={syncCursor}
    onclick={syncCursor}
    oninput={onInputHandler}
    oncompositionstart={() => (composing = true)}
    oncompositionend={() => {
      composing = false;
      syncCursor();
    }}
    onblur={() => (suppressAt = cursorPos)}
  />
{/if}

{#if popoverOpen && popoverPos}
  <CompletionPopover
    items={candidates}
    selectedIndex={selectionMoved ? selectedIndex : -1}
    left={popoverPos.left}
    top={popoverPos.top}
    onpick={confirmCompletion}
  />
{/if}

<style>
  textarea {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: ui-monospace, "Cascadia Code", "SF Mono", monospace;
    font-size: 0.82rem;
    resize: vertical;
  }
  input {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
  }
  textarea.invalid,
  input.invalid {
    border-color: var(--danger);
  }
</style>
```

- [ ] **Step 2: 型チェックを通す**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 3: コミット**

```bash
git add frontend/src/input/TqlCompletionField.svelte
git commit -m "feat: TqlCompletionFieldコンポーネントを追加"
```

---

### Task 6: `AddColumnModal.svelte` への組み込みと動作確認

**Files:**
- Modify: `frontend/src/ui/AddColumnModal.svelte`

**Interfaces:**
- Consumes: Task 5の `TqlCompletionField`（`mode`, `bind:value`, `placeholder`, `rows`, `invalid`, `oninput`, `lists`, `antennas`, `channels`）。

- [ ] **Step 1: import を追加する**

`frontend/src/ui/AddColumnModal.svelte:8` の直後に追加:

```svelte
  import TqlCompletionField from "../input/TqlCompletionField.svelte";
```

- [ ] **Step 2: エキスパートモードのtextareaを置き換える**

`frontend/src/ui/AddColumnModal.svelte:376-383` の `<textarea class="tql-input" ...></textarea>` を置き換える:

```svelte
        <TqlCompletionField
          mode="query"
          bind:value={tqlText}
          rows={4}
          placeholder={'from home, list("...") where has_files && !cw'}
          invalid={!!tqlErr}
          oninput={onTqlInput}
          {lists}
          {antennas}
          {channels}
        />
```

- [ ] **Step 3: 簡単モードのfilter inputを置き換える**

`frontend/src/ui/AddColumnModal.svelte:492-497` の `<input placeholder={"例: ..."} ...>` を置き換える:

```svelte
        <TqlCompletionField
          mode="predicate"
          bind:value={filterText}
          placeholder={"例: has_files && !cw && reactions >= 5"}
          invalid={!!filterErr}
          oninput={onFilterInput}
        />
```

- [ ] **Step 4: 不要になった `.tql-input` CSSルールを削除する**

`frontend/src/ui/AddColumnModal.svelte:624-636` の `.tql-input { ... }` と `.tql-input.invalid { ... }` ブロックを削除する（スタイルはTask 5でコンポーネント側に移した）。`input { ... }` / `input.invalid { ... }`（簡単モードの他の入力欄が引き続き使う）はそのまま残す。

- [ ] **Step 5: 型チェックを通す**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 6: 実機で動作確認する（`cargo tauri dev`）**

Run: `cargo tauri dev`

以下を手動確認する:
1. カラム追加 → 「エキスパート(TQL)」に切り替え、空欄で `fr` と打つと `from` が候補に出る
2. `from ` まで打つと `home`/`local`/.../`list` 等ソース名が候補に出る。`list` を選ぶと `list("` まで挿入されカーソルが引用符内に来る
3. 対象アカウントが実際に持つリスト名を選択中アカウントで開き、`list("` の続きにリスト名の一部を打つと実際のリスト名が候補に出て、選ぶと実IDが挿入され `")` まで補完される
4. `from home where ` まで打つとフィールド名（`has_files` 等）が候補に出る。`has_fi` まで打つと `has_files` に絞り込まれる
5. `has_files` を選んだ直後、`&&`/`||`/`contains` 等の演算子が候補に出る
6. 矢印キーで候補選択、Tab/Enterで確定、Escapeで候補だけ閉じることを確認する
7. 「簡単」モードのフィルタ欄でも同様にフィールド名/演算子の補完が効くことを確認する（`from`/`list(...)` は出ないこと）
8. 既存のバリデーションエラー表示（不正なTQLを打った時の赤枠＋エラーメッセージ）が補完機能追加後も引き続き動くことを確認する

- [ ] **Step 7: コミット**

```bash
git add frontend/src/ui/AddColumnModal.svelte
git commit -m "feat: AddColumnModalのTQL入力欄に補完を組み込む"
```
