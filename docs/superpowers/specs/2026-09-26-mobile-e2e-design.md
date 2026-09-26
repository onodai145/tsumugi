# モバイルUI E2E 設計（Issue #259）

## 背景・目的

`e2e/`（WebdriverIO + tauri-driver + WebKitGTK）はデスクトップUI前提のみで、モバイルUI（スマホレイアウト）を検証するE2Eが無い。
Issue #257（Edge-to-Edge対応）の確認はAVD + scrcpyでの手動目視だった。

目的: モバイルUI特有のレイアウト崩れ（セーフエリア被り、横はみ出し等）を、実機/エミュレータ無しで再現性をもって自動検出する。

## 方針

`store.svelte.ts` の `useMobileUi()` は設定 `uiMode: "mobile"` でデスクトップ上でもモバイルUIを強制できる（Issue #51）。
これを利用し、既存のtauri-driver E2E基盤（Misskey docker-compose・seed・wdio）上で、ウィンドウを縮小してモバイルレイアウトを検証する。

### 検討して採用しなかった案

- Appium + UiAutomator2 + AVD: 実機相当の挙動（Edge-to-Edge実inset、IME、ジェスチャー）まで検証できるが、構築・実行コストが大きくCIに載せにくい。別Issueとして将来検討。
- セーフエリア差分アサートのみ: 「レイアウト崩れ全般」を満たさない。

## 1. 本番コード変更: セーフエリアの変数化

WebKitGTKは `env(safe-area-inset-*)` に値を持たず常に0になるため、そのままではセーフエリア被りを検証できない。
`env()` を直接使う代わりにCSS変数を経由させ、E2Eから上書き可能にする。

`frontend/src/app.css` の `:root` に定義:

```css
:root {
  --safe-top: env(safe-area-inset-top, 0px);
  --safe-right: env(safe-area-inset-right, 0px);
  --safe-bottom: env(safe-area-inset-bottom, 0px);
  --safe-left: env(safe-area-inset-left, 0px);
}
```

- `env(safe-area-inset-*)` を直接使っている箇所（App, Modal, ConfirmDialog, MediaViewer, AppMenu, Backstage, Column 等）を `var(--safe-*)` に置換する。実機の見た目・挙動は変えない。
- 今後追加するUIも `var(--safe-*)` を使う規約とし、`docs/design/style-guide.md` に追記する。
- E2Eは `document.documentElement.style.setProperty('--safe-top', '47px')` で上書きする。

## 2. E2E構成

```
e2e/
├─ helpers/mobile.ts        # enableMobileMode(): uiMode=mobile設定 + 390x844へリサイズ + safe-area注入
├─ wdio.mobile.conf.ts      # specs-mobile/ を対象にする専用config
└─ specs-mobile/
    ├─ layout.e2e.ts        # FAB表示、投稿欄が常時表示でないこと、カラムが100%幅で横スナップすること
    ├─ safe-area.e2e.ts     # 注入したinset分だけ、FAB・投稿モーダル・下部メニューバーが内側に収まること。6vh/8vh（約51/67px）より大きいinset（モーダル用にtop 120、FABは非ゼロのright inset）を注入し、safe-areaが効いていなければ失敗するようにしている。メディアビューワーは Issue #384 で `safe-area-media.e2e.ts` として追加
    └─ overflow.e2e.ts      # `[data-columns-scroll]` の外側の各要素について `right <= innerWidth + 1` を確認する。ルートが overflow-hidden のため `documentElement.scrollWidth` では検出できないので、要素ごとの右端比較にしている
```

- 検証手法: `getBoundingClientRect()` を注入inset・ビューポート境界と比較する。スクリーンショット比較は使わない。
- 既存のテスト用Misskey（docker-compose）とseedを流用する。
- `specs/`（デスクトップ）とは別configにし、ウィンドウサイズ前提の干渉を避ける。`maxInstances: 1` は維持。
- `pnpm e2e:mobile` で実行する。
- CIでは `e2e` ジョブ内で `pnpm e2e` の直後に `pnpm e2e:mobile` を実行する（所要時間は約1分）。

## 3. 限界

デスクトップWebKitGTK上の近似であり、以下は対象外（引き続き実機確認）:

- Android WebView固有の挙動（Edge-to-Edgeの実inset値、IME、ジェスチャー）
- タッチ操作の実機での感触

## 4. テスト

- 本番コード変更（変数化）は `pnpm check` / `pnpm test` と、デスクトップE2Eが従来通り通ることで回帰確認する。
- 新規specがinsetを0に戻した状態（変数化前相当）で失敗することを一度確認し、検出力を担保する。
