# Nightly ReleaseのRelease notes自動生成 設計

Issue: #333

## 背景・現状の問題

`.github/workflows/nightly.yml`は毎晩`cleanup`ジョブで前回のnightly Release/タグを削除し、`build`(3プラットフォーム並列、`tauri-apps/tauri-action`)と`android`(`softprops/action-gh-release`)がそれぞれ新規にReleaseを作成・アセットをアップロードしている。

しかし、どちらのアクション呼び出しにも`releaseBody`/`generateReleaseNotes`(tauri-action)や`generate_release_notes`(action-gh-release)は指定されておらず、Release notesを明示的に生成する処理が存在しない。実際の`nightly`タグのReleaseを確認すると、直近の安定版タグ(`v0.10.0`)以降にmainへマージされた全PRを列挙した"What's Changed"形式の本文が付いているが、これは過去にGitHub Web UI等で一度手動生成されたものが`cleanup`の削除漏れ等により残存している可能性が高く、ワークフローが確実に・毎回生成しているものではない。

## 決定事項

- Release notesの範囲は**GitHub標準のauto-generate release notes機能**に任せる。比較対象タグも明示指定せず、GitHubの自動選択(直近の非prerelease公開版)に任せる。
- 実装は**専用の`release`ジョブを新設**し、`cleanup`の直後・`build`/`android`の前に、空のReleaseを`gh release create --generate-notes`で作成する。

### 採用理由

`build`(3プラットフォーム並列)と`android`は現状どちらも「タグに対応するReleaseが無ければ作成する」ロジックを持つ。これらを並列実行したまま両方に`generateReleaseNotes`相当のフラグを個別に追加すると、以下のレース条件が残る:

- `tauri-action`は`releaseDraft: false`の場合、Release不在なら`generate_release_notes`フラグ付きで新規作成する。
- `action-gh-release`(Windowsポータブルexe・Androidアセット用)も同様にRelease不在なら新規作成できるが、`generate_release_notes`を渡さない場合、後から作成が完了した側がnotes無しのReleaseを作ってしまう可能性がある。

`action-gh-release`のREADME(`dist/index.js`と同梱ドキュメント参照、pinned commit `7c4723f7a335432393329f8f1c564994ce50185d`)には以下の記載がある:

> 💡 When the release info keys (such as `name`, `body`, `prerelease`, etc.) are not explicitly set and there is already an existing release for the tag, the release will retain its original info.

つまり、Releaseを**先に**確定した状態(notes生成済み)で作っておけば、後続の`build`/`android`はどちらも「既存Releaseを見つけてアセットを追加するだけ」になり、notesを上書きしない。これにより生成ロジックを1箇所(`release`ジョブ)に集約でき、並列実行の順序に依存しなくなる。

`tauri-action`側も同様に、`releaseDraft: false`のときは`getReleaseByTag`で既存Releaseを取得するだけで、見つかった場合は本文の書き換えを行わない(`dist/index.js`の`getOrCreateRelease`実装で確認済み)。

## 変更内容

### `release`ジョブ(新設)

- `needs: [check, cleanup]`、`if: needs.check.outputs.should_run == 'true'`。
- 現在`build`ジョブの`meta`ステップにある日付・shortsha算出をこのジョブに移動し、`outputs`として公開する(`build`ジョブでの重複算出をなくすため)。
- 主処理:
  ```sh
  gh release create "$NIGHTLY_TAG" \
    --repo "${{ github.repository }}" \
    --title "tsumugi nightly (${date} @ ${shortsha})" \
    --target "${{ github.sha }}" \
    --prerelease \
    --generate-notes
  ```
  アセットは付与しない(空のReleaseを作るだけ)。

### `build`ジョブ

- `needs`に`release`を追加(`[check, cleanup, release]`)。
- `meta`ステップは削除し、`release`ジョブの`outputs`(date/shortsha)を参照するよう`tauri-action`の`releaseName`を書き換える。
- それ以外(`tagName`/`releaseCommitish`/`releaseDraft`/`prerelease`/`assetNamePattern`/`args`)は変更なし。

### `android`ジョブ

- `needs`に`release`を追加(`[check, cleanup, release]`)。
- それ以外は変更なし。

## テスト/検証方法

CI(GitHub Actions)の実際の挙動に依存する変更のため、ローカルでの単体テストは対象外。以下をワークフロー実行結果で確認する:

1. `workflow_dispatch`で手動トリガーし、`release`ジョブが単独で正常終了すること。
2. `build`(3プラットフォーム)・`android`のログに「既存Releaseを発見してアセットを追加した」旨のログが出ており、Release新規作成を試みていないこと。
3. 生成された`nightly` Releaseの本文が、直近の安定版タグからの妥当な差分一覧になっていること(無限に肥大化した一覧ではないこと)。
4. 通常の`release.yml`(バージョンリリース)には影響が無いこと(このIssueのスコープはnightlyのみで、`release.yml`は変更しない)。
