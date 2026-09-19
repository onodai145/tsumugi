# メディアビューワー移行 設計ドキュメント (Issue #295)

- 作成日: 2026-09-19
- 対象: `frontend/src/render/MediaGrid.svelte` が使用している `viewerjs` の置き換え
- 関連Issue: [#295](https://github.com/onodai145/tsumugi/issues/295)「Viewer.jsから移行する」

## 1. 背景・動機

現状 `MediaGrid.svelte` は画像のみを `viewerjs` の拡大ビューワー（ズーム/回転/反転/前後送り）で扱っており、動画・音声はグリッド内のネイティブ `controls` によるインライン再生のみで、拡大表示・統一的な前後送りには対応していない。

Issue #295 では移行先候補として Bigger Picture / lightGallery / pencere が挙げられていたが、調査の結果、画像・動画・音声すべてを扱いつつ回転/反転を標準搭載し、かつ MIT 相当の許諾なライセンスを満たす既製ライブラリは存在しないことが判明した（詳細は本ドキュメント末尾の「検討した代替案」参照）。そのため、**拡大ビューワーの殻（UI・ツールバー・前後送り・ライセンス）は自前実装し、実装コストの高い部分（ズーム/パンのジェスチャー座標計算、動画・音声のプレーヤーUI）のみ実績のある外部ライブラリに委譲する**方針とする。

## 2. スコープ

- 画像・動画・音声すべてを1つの拡大ビューワーで前後送りしながら閲覧できるようにする。
- 画像: ズーム・パン・回転・反転・ダウンロード。
- 動画: ズーム・パン（ズーム時のみパン有効）・Vidstackによる再生コントロール・ダウンロード。
- 音声: Vidstackによる再生コントロール・ダウンロード（ズーム/回転は対象外）。
- 閲覧注意（センシティブ）ファイルも前後送り対象に含め、未表示のまま到達したらビューワー内でカバー表示し、クリックで表示する。
- `viewerjs` 依存を削除する。
- 対象外: その他ファイル（📄 表示のもの）は現状通り `openUrl` で外部に開き、ビューワーの対象にしない。

## 3. アーキテクチャ

### 3.1 新規コンポーネント: `MediaViewer.svelte`

`frontend/src/render/MediaViewer.svelte` を新規追加する。

- **表示方式**: `ui/Modal.svelte` と同様の `portal` パターン（`document.body` 直下にマウント）でフルスクリーンオーバーレイ表示する。汎用 `Modal.svelte` は流用せず、セーフエリア対応込みのフルブリード専用レイアウトを組む（現状 `MediaGrid.svelte` の `:global(.viewer-container ...)` にあったセーフエリアCSSの考え方を踏襲）。
- **props**: `files: DriveFile[]`（グリッドの全ファイル配列）、`startIndex: number`、`revealed: Record<string, boolean>`（`bind:revealed` で `MediaGrid.svelte` と双方向同期）、`onclose: () => void`。
- **対象アイテム**: `viewItems` は `files` のうち画像・動画・音声のみ（📄その他ファイルは除外）。閲覧注意の有無に関わらず全件を対象にする。
- **状態**: `currentIndex`（表示中の位置）。画像用に `rotation`（0/90/180/270 の4値）・`flipH`/`flipV`（真偽値）を保持し、アイテム切替時にリセットする。ズーム/パン倍率は `@panzoom/panzoom` インスタンスが保持し、アイテム切替時に `panzoom.reset()` する。

### 3.2 閲覧注意ゲーティング

- ビューワーで `viewItems[currentIndex]` の `revealed[id]` が `false` の場合、ツールバー・パンズーム・プレーヤーを初期化せず、`MediaGrid.svelte` のグリッド内カバーと同じ文言（「閲覧注意（クリックで表示）」）のカバー画面を表示する。
- カバーをクリックすると `revealed[id] = true` に更新する。この状態は props 経由で `MediaGrid.svelte` 側の `revealed` にも反映され（`bind:revealed`）、ビューワーを閉じてグリッドに戻った際も表示済み状態が維持される。

### 3.3 起動トリガー

- **画像**: 現状通りクリックでビューワーを起動（起動時 `startIndex` はクリックした画像の `viewItems` 内でのインデックス）。
- **動画・音声**: グリッド内では現状通りネイティブ `<video controls>` / `<audio controls>` でインライン再生を維持する。ネイティブコントロールとのクリック競合を避けるため、既存のダウンロードボタン（💾）と並べて「拡大」ボタンを追加し、そこから明示的にビューワーを起動する。

### 3.4 画像のズーム・パン・回転・反転

- [Cropper.js v2](https://fengyuanchen.github.io/cropperjs/)（`fengyuanchen/cropperjs`、MIT、Viewer.jsと同じ作者によるWeb Componentsベースの後継プロダクト）を採用する。クロップ枠（`<cropper-selection>`）は使わず、`<cropper-canvas>` + `<cropper-image>` のみを組み合わせた「ビューワー構成」で画像を表示する。
- `<cropper-image>` の `$rotate(angle)` / `$scale(x, y)`（負値で反転） / `$zoom(scale, x, y)` / `$translate(x, y)` メソッドで、ズーム・パン・回転・反転を単一のAPIで完結させる。属性 `rotatable` / `scalable` / `translatable` で各操作を有効化する。panzoomのような別ライブラリとの transform 合成は不要になる。
- ツールバー: ズームイン / ズームアウト / 等倍リセット（`$resetTransform()`） / 左回転 / 右回転 / 左右反転 / 上下反転 / ダウンロード / 閉じる。
- ピンチズームの挙動は公式ドキュメント上で明記が確認できなかったため、実装の最初に実機（Android含む）で動作検証する（詳細は3.9節）。期待通り動作しない場合は `@panzoom/panzoom` へのフォールバックを検討する（本ドキュメント末尾の代替案参照）。

### 3.5 動画のズーム・パン

- 動画も `@panzoom/panzoom` の対象にするが、ネイティブ操作・Vidstackのコントロールバーとの誤操作競合を避けるため、**単一ポインタでのドラッグパンは `scale > 1`（ズーム済み）の時のみ有効化**し、等倍時は無効化する（`panzoom` の `disablePan` オプションを動的に切り替える）。ピンチズーム・ホイールズームは常時有効。
- 回転・反転ボタンは動画には表示しない（Issue検討の結果、不要と判断）。

### 3.9 パン/スワイプ操作の競合解決（重要）

過去のIssue #296対応（[[touch-action-pan-y-pointercancel-native-scroll-conflict]]、PR #328）で、「JSでpointerdown/pointermoveを検知してpreventDefaultする自前ドラッグ方式の横スワイプ」は実機（Android）で`pointercancel`によりブラウザのネイティブジェスチャーに奪われ機能しないという教訓を得ている。今回のビューワーでも同種の競合が2箇所で起こりうるため、以下の方針で回避する。

- **画像の等倍時パン と 前後送りスワイプの競合**: `<cropper-image>` の `translatable` 属性は **`scale > 1`（ズーム済み）の時のみ有効化**し、等倍時は無効化する（3.5節の動画と同じ方針に統一）。等倍時の横ドラッグは cropper-image に奪われず、前後送りスワイプ側に委ねられる。`scale` は cropper-image の `transform` イベント（`detail.matrix`）を購読して判定する。
- **前後送りスワイプの実装方式**: JSのpointerドラッグ横取り方式は採用せず、**CSS Scroll Snap**（`scroll-snap-type: x mandatory` を持つ横スクロールコンテナに `viewItems` を1アイテム1ページとして並べ、各ページに `scroll-snap-align: start` を設定）で実装する。ブラウザのネイティブスクロールとして扱われるため、`pointercancel` によるジェスチャー横取り競合が原理的に起こらない。スクロール停止（`scrollend`イベント、非対応環境向けにdebounceされた`scroll`イベントでフォールバック）を検知して `currentIndex` を更新する。矢印ボタン・キーボード操作は対象ページへ `scrollTo({ behavior: "smooth" })` する。
- 上記の「`scale > 1`でのみtranslatable」方針が実機で機能するか（等倍時にcropper-canvasがポインタイベントを完全に外側のScroll Snapコンテナへ透過させるか）は未検証のため、3.4節のピンチズーム検証と合わせて実装最初のスパイクで確認する。機能しない場合は、画像表示中のみ外側コンテナの `overflow-x` を一時的に無効化し前後送りを矢印ボタン/キーボードのみに制限するなどのフォールバックを検討する。

### 3.10 Vidstackのテーマ連携

`vidstack/player/styles/default/theme.css` はデフォルトパレット（例: `--video-brand`, `--audio-brand`, `--video-focus-ring-color`, `--video-border-radius` 等のCSS変数）を持つが、`docs/design/style-guide.md` の規約（色はCSS変数を直書きせず`app.css`のトークン経由、角丸はスケールに準拠）に従い、以下をアプリのトークンにマッピングする。

```css
media-player {
  --video-brand: var(--accent);
  --audio-brand: var(--accent);
  --video-focus-ring-color: var(--accent);
  --audio-focus-ring-color: var(--accent);
  --video-border-radius: var(--radius-md); /* style-guide.md §2 のmdスケールに合わせる */
  --audio-border-radius: var(--radius-md);
}
```

ユーザーカスタムテーマ・プリセットが `--accent` 等を上書きする前提と矛盾しないよう、Vidstack側の色をハードコードせず必ずこのマッピング経由にする。

### 3.6 動画・音声プレーヤー: Vidstack

- 動画・音声の再生UI（再生/一時停止・シークバー・音量・再生速度変更・フルスクリーン）は自前実装せず、[Vidstack](https://vidstack.io/)（MIT、Web Components ベース、公式Svelte対応）を採用する。
- Vidstack は既存の `<video>` / `<audio>` 要素を `<media-player>` でラップする progressive enhancement 方式のため、現状のメディアソース周りのロジック（`f.url` 等）をそのまま活かせる。
- Vidstack はプロバイダを差し替え可能な設計になっており、将来的にWASMデコーダ（特定コーデックがWebKitGTKでネイティブ再生できない場合など）を組み込みたくなった際も、Vidstackのプロバイダ拡張ポイント経由で対応できる余地がある。今回はプロバイダのカスタム実装までは行わず、標準のネイティブプロバイダ（`<video>`/`<audio>` ラップ）のみを使用する。
- 選定理由: Plyr・media-chrome・Vidstackの開発チームは統合し、Mux主導でVideo.js v10を共同開発する方針を表明しており（[議論スレッド](https://github.com/vidstack/player/discussions/1747)）、Vidstackは実質的にPlyrの後継に位置づけられている。star数はPlyrに劣るが、公式Svelteインストールガイドがあり、エンジン差し替えの拡張性でも優れる。

### 3.7 前後送りUI

以下を並存させる（互いに排他ではない）:

- 矢印ボタン（画面左右に固定表示、対象ページへ `scrollTo({ behavior: "smooth" })`）
- 画面左右の透明クリックエリア（ホバー時に矢印アイコンを表示、クリックで同上）
- キーボード ← / →（同上）
- 横スワイプ（3.9節のCSS Scroll Snapによるネイティブスクロール。JSのpointerドラッグ横取りは使わない）

### 3.8 共通操作

- Esc キー / 背景クリックで閉じる。
- セーフエリア対応: 閉じるボタン（右上）・フッター（ツールバー等、画面下端）を `env(safe-area-inset-*)` 分だけ内側にずらす（Issue #331 対応の踏襲、`MediaGrid.svelte` に現存する `:global(.viewer-container ...)` の考え方をそのまま新コンポーネントに移植）。

## 4. データフロー

```
MediaGrid.svelte
  - files: DriveFile[]
  - revealed: Record<string, boolean>（$state）
  - クリック/拡大ボタン → MediaViewer を起動（startIndex, files, bind:revealed）

MediaViewer.svelte
  - viewItems = files.filter(画像/動画/音声)
  - currentIndex（$state）、Scroll Snapコンテナのscroll位置と同期
  - revealed[viewItems[currentIndex].id] が false → カバー表示、クリックで revealed 更新（親に伝播）
  - true → 画像: <cropper-canvas> + <cropper-image>（scale>1でのみtranslatable）
           動画/音声: Vidstack <media-player> (+動画のみpanzoom、scale>1でのみpan)
  - 前後送り操作（矢印/クリックエリア/キーボード/Scroll Snapスワイプ） → currentIndex 更新 → rotation/flip/zoom リセット
```

### 4.1 テスト可能なロジックの分離

custom elements（`<cropper-image>` / `<media-player>`）はjsdom環境で意味のある初期化ができない（`ResizeObserver` / Web Animations API / shadow DOM挙動を欠く）ため、Vitestで検証可能な純粋ロジックは `lib/mediaViewer.svelte.ts` に切り出す（既存の `lib/profileModal.svelte.ts` + `lib/profileModal.svelte.test.ts` と同じ構成パターンを踏襲）。

- `viewItems` の算出（`DriveFile[]` → 画像/動画/音声のみ抽出）
- `currentIndex` の次/前/wrap挙動
- 閲覧注意ゲーティング（`revealed[id]` 参照・更新）
- 画像の rotation/flip 状態とアイテム切替時のリセット

custom elementsに直接紐づく部分（Cropper.js/Vidstackの実際のズーム・回転・再生動作）はVitestでは検証せず、3.11節の実機確認に委ねる。

## 5. エラーハンドリング

- メディア読み込み失敗時（画像 `onerror` / Vidstackのエラーイベント）は、インラインでエラーメッセージを表示し、閉じる・次へ送るための導線を残す（現状の `viewerjs` にはなかった明示的なエラーUI）。
- Fullscreen API の呼び出し失敗（環境によって拒否されるケースがある）は例外を握りつぶし、`app.reportError` でログするに留め、UIをクラッシュさせない。
- `panzoom` インスタンスは `onMount` のクリーンアップ、およびアイテム切替時に確実に破棄・再生成し、リスナーのリークを防ぐ（現状の `viewer?.destroy()` パターンを踏襲）。

## 6. テスト方針

- `frontend/src/lib/mediaViewer.svelte.ts`（4.1節）をVitestで単体テスト: `viewItems`算出、`currentIndex`の次/前/wrap、閲覧注意ゲーティング、rotation/flipのリセットロジックを実アサーションで検証する。
- `frontend/src/render/MediaViewer.svelte` を `@testing-library/svelte` でコンポーネントテスト（custom elements内部の描画は検証対象外、DOM構造とイベント配線のみ）:
  - 矢印ボタン・キーボード操作で `currentIndex` が変わること（Scroll Snapのネイティブスクロール自体はjsdomで検証できないため、`scrollTo`呼び出しの発生や`currentIndex`状態の更新までを確認）
  - 閲覧注意アイテムに到達した場合にカバーが表示され、クリックで `revealed` が更新され親に伝播すること
  - Esc / 背景クリックで `onclose` が呼ばれること
- 実機確認（`cargo tauri dev`、詳細手順は3.11節）: 画像のピンチズーム・パン/スワイプ競合・回転・反転、動画・音声のVidstackプレーヤー操作、Misskeyホストの実動画/音声ファイルでの再生（GStreamerコーデック依存の確認）、閲覧注意ゲーティング、セーフエリア表示、Vidstackテーマ連携を目視確認する。

### 3.11 実機確認手順

- Linux/Xvfb環境: `Xvfb`を起動しつつ、実画面への描画漏れを防ぐため **`DISPLAY`をXvfbに向けるだけでなく`WAYLAND_DISPLAY`を明示的にunsetする**（Waylandセッション上ではDISPLAY設定だけでは不十分、[[dev-server-verification-must-use-virtual-display]]参照）。`WEBKIT_DISABLE_DMABUF_RENDERER=1`は`src-tauri/src/main.rs`が既定で設定済み。
- 動画・音声の検証は `vidstack.io` のデモファイルだけでなく、**実際にMisskeyインスタンスへアップロードしたmp4/webm/mp3ファイル**で行う（WebKitGTKの`<video>`/`<audio>`再生は最終的にシステムのGStreamerプラグインに依存するため、デモファイルで動いてもインスタンス側のエンコード設定次第で実際は再生できないケースがありうる）。
- 検証後、自分で起動した`cargo tauri dev`は正確なPIDを確認した上で終了する（`pkill`/`killall`は使わない）。

## 7. 依存関係の変更

- 削除: `viewerjs`
- 追加: `cropperjs`（v2、画像のズーム/パン/回転/反転）、Vidstack（`@vidstack/player` 系パッケージ、動画・音声プレーヤー、Svelte向け配布形態は実装時に最新ドキュメントで確認する）、動画のズーム/パンには引き続き `@panzoom/panzoom` を使用する（Cropper.js v2は画像専用で動画には非対応のため）

## 8. 検討した代替案

Issue本文の候補3つに加え、以下を調査した上で自前実装 + 部分的ライブラリ利用の方針に至った。

| ライブラリ | 画像 | 動画 | 音声 | 回転/反転 | ライセンス | 備考 |
|---|---|---|---|---|---|---|
| Bigger Picture (henrygd) | ○ | ○ | ○ | ✗（標準機能なし） | MIT | 作者が「バグ修正のみ」と明言、star 331 |
| Bigger Picture (TwistedAndyフォーク) | ○ | ○ | ○ | ✗ | MIT | 本家より15コミット進むがstar 18の個人フォーク |
| lightGallery + lg-rotate | ○ | ○ | ✗ | ○ | **GPLv3 or 商用有償** | MITプロジェクトへの組み込みはライセンス上の懸念あり |
| pencere | ○ | ○ | ✗ | ✗ | MIT | Svelte公式アダプタありだがEarly development、star 46 |
| PhotoSwipe | ○ | ○(公式プラグイン) | ✗（自前実装要） | ✗（自前実装要） | MIT | star 25.3k、公式`registerElement` APIあり |
| GLightbox | ○ | ○ | 未確認 | ✗ | MIT | 最終pushから9ヶ月、実績・機能ともに見劣り |
| Fancybox | ○ | ○ | ○ | ○ | **GPLv3 or 商用有償** | 市場シェア最大級だがライセンス懸念は同上 |

音声ネイティブ対応とrotate/flip標準搭載を両立し、かつ許諾ライセンスを満たす単一ライブラリは存在しなかったため、UI・ツールバー・前後送り・閲覧注意ゲーティングは自前実装しつつ、パーツごとに実績のある外部ライブラリへ委譲する方針とした。

画像のズーム/パン/回転/反転については当初 `@panzoom/panzoom`（ズーム/パン）+ 自前CSS transform（回転/反転）の組み合わせを想定していたが、Viewer.jsと同じ作者による [Cropper.js v2](https://fengyuanchen.github.io/cropperjs/) がクロップ枠なしの「ビューワー構成」でズーム・パン・回転・反転を単一APIで提供していることが判明したため、こちらに一本化した（詳細は3.4節）。動画のズーム/パンには画像専用のCropper.js v2が使えないため、引き続き `@panzoom/panzoom` を使用する。
