# モバイルE2E セーフエリア検証の拡充 設計（Issue #384）

## 背景・目的

PR #382（Issue #259）で追加した `e2e/specs-mobile/safe-area.e2e.ts` は、FAB・投稿モーダル上端・下部メニューバーしか検証していない。以下が未カバー:

- モバイル時のカラム領域上端のステータスバー被り（Issue #257の元凶。`App.svelte` の `main` の `pt-[var(--safe-top)]`）
- 左右inset（`--safe-left` / `--safe-right`。横向き時のノッチ想定）
- メディアビューワーのセーフエリア（画像アップロード用のE2Eヘルパーが無かった）

これらを追加し、#257相当のリグレッションを自動検出できるようにする。設計の前提・手法は `2026-09-26-mobile-e2e-design.md` を踏襲する（`uiMode=mobile` + 390x844 + `--safe-*` 疑似注入、`getBoundingClientRect()` 比較）。

## 1. テスト用Misskeyヘルパー（`e2e/helpers/misskeyApi.ts`）

- `uploadImage(token: string): Promise<string>` — 1x1 PNG（base64をインラインで保持）を `/api/drive/files/create`（multipart/form-data）でアップロードし、ファイルidを返す。
- `createNote(token, text, fileIds?: string[])` — 第3引数を任意で追加し、指定時のみ `fileIds` をリクエストに含める。既存の呼び出しは影響を受けない。

## 2. モバイル用ヘルパー（`e2e/helpers/mobile.ts`）

- `addHomeColumn(): Promise<void>` — メニュー → カラム追加 → 送信 → `.column-root` の表示待ち。`layout.e2e.ts` のインライン手順と同じ内容。既存specは変更しない（新規specのみが使う）。

## 3. 既存 `safe-area.e2e.ts` の拡張（画像不要）

- `before` で `addHomeColumn()` を呼ぶ（既存3テストへの影響が無いことを実行で確認する）。
- **上端**: `.column-root` の `top >= SAFE_AREA.top`（47）。`pt-[var(--safe-top)]` を外すと top が0になり失敗する。
- **左右**: `left=30, right=30` を注入し、`app-menu-trigger` の left が30以上であることを確認する。既存パディング（`max(8px, var(--safe-left))`）より大きい値のため、変数が効いていなければ失敗する。
- 注入したinsetは既存テスト同様、try/finallyで必ず `SAFE_AREA` に戻す。

## 4. 新規 `safe-area-media.e2e.ts`

- `before`: シード管理者トークンで `uploadImage` → `createNote(token, text, [fileId])` → アカウント追加 → `addHomeColumn()` → 画像セルをクリックしてビューワーを開く。
- **上端**: 閉じるボタン（`aria-label="閉じる"`）の top が `SAFE_AREA.top` 以上。
- **右端**: 右insetを非ゼロ（例24）で注入し、閉じるボタンの right が `vw - 24` 以下。
- **下端**: 画像ツールバー（`aria-label="画像ツールバー"`）の bottom が `vh - SAFE_AREA.bottom` 以下。
- 別ファイルにするのは、画像準備の失敗リスクを既存specから切り離すため。

## 5. 検出力の担保

各テストについて、対応する本番の `var(--safe-*)` を一時的に外して RED を確認し、戻して GREEN を確認する（PR #382と同じ手順）。本番ファイルの変更はコミットに含めない。

## 6. リスクと扱い

- WebKitGTKが自己署名CAを信頼して画像を読み込めるかは未確認。ビューワーの枠（閉じるボタン行・ツールバー）の位置は画像の読み込み成否に依存しない想定だが、ツールバーの表示条件は実装時に実機で確認する。
- 画像が読み込めない・ツールバーが出ない場合は原因を特定する。アサーションを弱めて通すことはしない。解決できなければBLOCKEDとして相談する。

## 7. 限界

デスクトップWebKitGTK上の近似である点は #382 と同じ（Android WebView固有の挙動は対象外）。
