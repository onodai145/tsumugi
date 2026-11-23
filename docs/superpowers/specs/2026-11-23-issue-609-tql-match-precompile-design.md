# TQL `match` の正規表現を事前コンパイルする設計 (Issue #609)

## 背景と目的

`filter/eval.rs` の `match` は、ライブ受信のノートごとに `regex::Regex::new(...)` でパターンをコンパイルし直している。`CompiledFilter` は AST だけを保持し、Regex を保持しない。正確性には影響しないが、ストリーミングで流れるノートのたびに同じパターンをコンパイルするのは無駄である。

`check_compare` は右辺にフィールドも許す(`text match user.username` など)ので、評価結果の単純なキャッシュはできない。右辺が**文字列リテラルのときだけ**、フィルタの生成時に一度コンパイルして保持する。

## 決定事項

- 方式 A を採る: `CompiledFilter::Tql` のバリアントに事前コンパイルした Regex の表を持たせる。グローバルなキャッシュ(方式 B)は採らない。
- 右辺が動的な `match`(フィールドなど)は、現状どおりノートごとにコンパイルする。LRU は入れない(YAGNI)。
- 評価の結果は変えない。不正なリテラルのパターンは常に不一致(インメモリ評価・SQLite・PostgreSQL・MySQL で既に揃っている、Issue #608)。
- `CompiledFilter` を作る経路を一つにする(`CompiledFilter::from_expr`)。事前コンパイルの取りこぼしを防ぐため。

## 目標 / 非目標

目標:

- リテラルの `match` パターンが、`CompiledFilter` の生成時に一度だけコンパイルされる。ノートごとの `Regex::new` が無くなる。
- 同じパターンが式木に複数あっても、コンパイルは一度。
- 既存の評価結果(不正なパターン、`!` との組み合わせを含む)が変わらない。

非目標:

- 右辺が動的な `match` の最適化。
- SQLite の `regexp()` 関数(`store/db.rs`)の最適化。これは SQL の実行側で、別の問題である。
- `filter/mute.rs` の単語ミュート(すでに `WordMuteRule::Regex` が事前コンパイルされている)。
- SQL 射影(`filter/sql.rs`)の変更。AST は変えない。

## 構成

### `filter/eval.rs`

```rust
/// `match` の右辺がリテラルのパターンを、事前にコンパイルした結果。
/// 不正なパターンは `None`(評価は常に不一致)。
/// 作れるのは `compile_regexes` だけ(中身は非公開の newtype)。空の表を持つ `CompiledFilter::Tql` を
/// `filter::eval` の外から書けないようにして、事前コンパイルの取りこぼしを防ぐ。
#[derive(Clone)]
pub struct RegexTable(HashMap<String, Option<Regex>>);

pub fn compile_regexes(expr: &Expr) -> RegexTable;
pub fn evaluate(expr: &Expr, note: &Note, ctx: &EvalContext) -> bool;          // 既存。表なしで評価する
pub fn evaluate_with(expr: &Expr, note: &Note, ctx: &EvalContext, re: &RegexTable) -> bool;
```

- `compile_regexes` は式木を走査し、`Compare { op: Match, rhs: Value::Str(p) }` のパターンを集める。キーはパターン文字列なので、同じパターンは一つにまとまる。
- `evaluate_with` の `Match` は、右辺が `Value::Str(p)` で表に `p` があれば、その Regex を使う。表に無い、または右辺が動的なら、これまでどおりその場でコンパイルする。
- `evaluate` は空の表で `evaluate_with` を呼ぶ。シグネチャは変えない(既存のテストと、表を持たない呼び出し元のため)。

### `filter/mod.rs`

```rust
pub enum CompiledFilter {
    PassAll,
    Keywords(Vec<String>),
    Tql { expr: ast::Expr, regexes: eval::RegexTable },
}

impl CompiledFilter {
    /// AST から TQL のフィルタを作る。事前コンパイルはここだけで行う。
    pub fn from_expr(expr: ast::Expr) -> Self;
}
```

- `compile` の TQL 分岐は `parse_predicate` の結果を `from_expr` に渡す。
- `matches` は `evaluate_with(expr, note, ctx, regexes)` を呼ぶ。
- `Regex` は `Clone`(内部は `Arc`)なので、`CompiledFilter` の `#[derive(Clone)]` はそのまま使える。

### `commands/column.rs`(機械的な変更)

- `column.rs:921`: `CompiledFilter::Tql(expr)` を `CompiledFilter::from_expr(expr)` に変える。`from … where …` 形式のクエリは `compile` を通らず、ここで直接構築されているので、ここを直さないと最も主要な経路で事前コンパイルが効かない。
- `column.rs:1953` / `column.rs:2277`: パターンを `CompiledFilter::Tql { expr, .. }` に変える。

## テスト

`filter/mod.rs`(作成済み・実装前は赤):

- リテラルのパターンは、同じものが一つにまとまって事前コンパイルされる(表の大きさで確認)。
- 不正なリテラルのパターンは表に `None` で入り、常に不一致。`!` を付けると一致する。
- 右辺が動的な `match` は表に入らず、ノートごとの評価で正しく動く。

`commands/column.rs`:

- `from … where text match "…"` の経路で作られた `CompiledFilter` の表にパターンが入っている(`from_expr` を通ることの確認)。

既存の `filter::eval` の `match` のテスト(`(?i)` など)は、変更なしで通ること。

## 影響範囲とリスク

- 型を変えるのは `CompiledFilter::Tql` だけ。ビルドが通れば、取りこぼしはコンパイラが検出する(構築・パターンのすべてがエラーになる)。
- 評価の結果は変わらない。事前コンパイルは `Regex::new(p)` の呼び出しを前倒しするだけで、設定(サイズ上限など)は同じ。
- メモリ: カラムごとに、リテラルのパターンの数だけ Regex を保持する。ユーザーが書く TQL のパターン数は少ない。
- 実測は行わない(Issue は「正確性には影響しない」性能の改善で、優先度は低い)。必要なら別途ベンチを取る。

## ドキュメントの更新

- `CLAUDE.md` の `filter/` の説明: `CompiledFilter` がリテラルの `match` を生成時に事前コンパイルすることと、`CompiledFilter::Tql` を直接構築せず `from_expr` を使うことを追記する。
- `docs/design/filter-dsl-design.md` のインメモリ評価の節: 同じく事前コンパイルについて一文追記する。
