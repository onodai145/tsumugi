# Rust のコードを rustfmt で整形し、CI でチェックする(Issue #672)

## 目的

`src-tauri/` の整形を rustfmt の既定に揃え、以後の崩れを CI で検出する。ロジックは変えない。

成功条件:

- 整形だけのコミットになっていて、手作業の変更が混ざっていない。
- `cargo test` が通る。
- `git blame` で整形コミットを飛ばせる。
- 未整形のコードを含む PR は CI で落ちる。

## 現状(2026-11-26、main = 87b14bc)

- `rustfmt.toml` も CI のチェックも無い。
- `cargo fmt --check` の差分は 110 ファイル、1904 箇所。rustfmt 1.10.0-stable と 1.12.0-nightly の出力は同じ。
- `src-tauri/llama_target.rs` は `build.rs` と `src/translate/mod.rs` が `include!` で読むため、`cargo fmt` の対象に入らず、未整形のまま残る。他に漏れるファイルは無い(追跡中の全 `.rs` を個別に確認した)。
- `plugins/tauri-plugin-haptics` は整形済み。
- 未マージの PR で `.rs` を変更するのは #510 だけ。

## 決定事項

### 設定は既定のまま(`rustfmt.toml` を作らない)

Rust Style Guide は、開発者とツールが既定のスタイルに従うことを推奨している。差分は `max_width = 120` と `use_small_heuristics = "Max"` を併用した場合(+2682 / -3375)より大きく、+13343 / -3235 になる。差分の大きさは一度きりで、blame への影響は `.git-blame-ignore-revs` で減らせる(新しく増えた行には残ることがある)ので、標準に揃えるほうを取る。

### `llama_target.rs` を `cargo fmt` の対象にする

`build.rs` の `include!("llama_target.rs")` を `mod llama_target;` に変える。`cargo fmt` は `mod` 宣言をたどるので、このファイルだけの `rustfmt` 呼び出し(CI・pre-commit・`CLAUDE.md`)と、`--edition` の直書きが不要になる。`src/translate/mod.rs` のテストの `include!` は変えない。判定の中身は変えない。

以下の節にある `rustfmt ... llama_target.rs` の実行は、この変更の後は行わない(整形コミットを作った時点の手順として残す)。

### コミット構成

ブランチ `chore/issue-672-rustfmt`、PR は 1 本。

1. この spec と plan。
2. 整形だけ: `src-tauri/` で `cargo +stable fmt --all` と `rustfmt +stable --edition 2021 llama_target.rs`。
3. `.git-blame-ignore-revs` に 2 の hash を登録する。
4. CI のチェックと `CLAUDE.md` の追記。
5. pre-commit のフック。
6. `llama_target.rs` を `mod` で読み込み、個別の `rustfmt` 呼び出しを消す。

### CI

`.github/workflows/test.yml` の `rust-test` ジョブに足す。`rust-test` は main の必須ステータスチェックなので、ブランチ保護の設定を変えずに強制できる。

- 整形崩れを早く落とすため、`dtolnay/rust-toolchain@stable` を apt のインストールより前に移し、`components: rustfmt` を指定する。
- 直後に `src-tauri/` で `cargo fmt --all --check` と `rustfmt --edition 2021 --check llama_target.rs` を実行する。
- 他のステップと同じく `needs.changes.outputs.code == 'true'` のときだけ実行する。

stable は版を固定していないので、将来の rustfmt の更新で、コードを変えていない PR が落ちる可能性がある。起きた時点で整形し直し、版は固定しない。

### CLAUDE.md

次の 2 点を書く。

- コミット前の整形コマンド(`llama_target.rs` は別に整形すること)。
- 手元で 1 回必要な `git config blame.ignoreRevsFile .git-blame-ignore-revs`(GitHub はファイルを自動で読む)。

### pre-commit

`.pre-commit-config.yaml` に `repo: local` のフックを足す(外部のフック用リポジトリは増やさない)。

- CI と同じ 2 つのコマンドを `+stable` で実行する(rustup が前提)。
- 確認するだけで、ファイルは書き換えない。落ちたら整形コマンドを実行して、ステージし直す。
- `.rs` がステージされたコミットだけで動く。

## 検証

- `llama_target.rs` だけを崩すと、`cargo fmt --all --check` が失敗する。`cargo test` が通り、テストの件数が変わらない(cfg `llama_target` が立ったまま)。

- 整形コミットの親に同じコマンドをかけた結果が、整形コミットのツリーと一致する。
- `cargo test` が通り、実行後の作業ツリーに差分が無い(`tauri.gen.ts` が変わらない)。
- 本体・plugin・`llama_target.rs` をそれぞれ一時的に崩すと、CI と同じコマンドが失敗する。plugin が `--all` で拾えない場合は、plugin 用のチェックを別に足す。
- `.git-blame-ignore-revs` を指定した `git blame` で、整形コミットに残る行が指定なしより減る。整形で新しく増えた行は元のコミットに戻せないことがあるので、残る行数を測って PR に書く。
- Android のコンパイルは CI の `android-build` で確認する。
- `pre-commit run rustfmt --all-files` が、整形済みのツリーで通り、崩したコードで落ちて、ファイルを書き換えない。

## マージと影響

- hash を保つため `gh pr merge --merge` でマージする。squash や rebase をすると `.git-blame-ignore-revs` の hash がずれる。push 後にブランチを rebase した場合は、hash を登録し直す。
- #510 は待たない。コンフリクトは #510 側で、整形してから main を取り込んで解消する。

## やらないこと

- 整形と同時のリファクタリングや clippy の指摘への対応。
- フロントエンドの整形。
