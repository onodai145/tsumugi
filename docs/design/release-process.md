# バージョニング / リリースプロセス

SemVer（`MAJOR.MINOR.PATCH`）を採用。`v1.0.0` 未満は `MINOR` 更新でも破壊的変更を許容する。
バージョンは `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` / `frontend/package.json` / `src-tauri/Cargo.lock`（`cargo update -p tsumugi`）の4箇所を揃えて更新する。

CHANGELOG は [git-cliff](https://git-cliff.org/) で Conventional Commits（`feat:` / `fix:` / `docs:` 等）から自動生成する（設定: [`cliff.toml`](../../cliff.toml)）。

```sh
git tag vX.Y.Z
git-cliff -o CHANGELOG.md   # 全タグ分を再生成
```

上記のバージョン更新・CHANGELOG生成・`release/vX.Y.Z` ブランチ作成 + コミットは `scripts/release.sh` で自動化できる（PR作成・マージ・タグpushは引き続き手動）。

```sh
scripts/release.sh X.Y.Z
```

`scripts/release.sh` はバージョン番号の更新に合わせて、`README.md` のOS別ダウンロードリンク（`<!-- release-download-links:start -->` 〜 `:end -->` の範囲）も新バージョンへ書き換える。

手順の全体（PR作成・マージ・タグpush・ドラフトリリースの手動公開まで）は [`CLAUDE.md`](../../CLAUDE.md) の「Release process」セクションを参照。
