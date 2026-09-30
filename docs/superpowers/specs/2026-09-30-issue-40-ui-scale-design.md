# UIスケール設定（Issue #40）

## 背景

Issue #40「UIをスケーリングできるようにする」。UI全体（文字・余白・アイコン・画像・カラム幅）をまとめて拡大縮小できるようにしたい。文字サイズだけの変更ではない。

## 決定事項と前提

- 対象はデスクトップ（Linux / macOS / Windows）のみ。Androidは別Issueに切り出す。
  - 理由: 採用するTauriの`Webview::set_zoom`はAndroid非対応（`tauri-2.11.5/src/webview/mod.rs`のdocコメントに "Android: Not supported"）。Androidの代替（CSS `zoom`）は実機確認が必要で、今回は含めない。
- ショートカット（Ctrl+`+`/`-`/`0`）、ライブプレビュー、カラム幅の自動補正はスコープ外。
- 拡大するとカラム幅もpxのまま拡大され、同時に見えるカラム数は減る。これは期待どおりの挙動として扱う。

## 検討した案

- **WebViewズーム（採用）**: `getCurrentWebview().setZoom(scale)`。ブラウザのCtrl+±と同じ仕組みで、px/remが混在していても全体が整合して拡縮する。デスクトップのみ。
- CSS `zoom`: Androidでも動く見込みだが、ポップオーバー位置・`100vh`・座標計算にずれが出やすく、全画面の検証が要る。今回は採らない。
- ルートfont-size(rem)変更: 文字しか拡縮せず、px指定の余白・アイコンが追従しないため、「UI全体」の要件を満たさない。

## 設計

### 設定値

- `UiPrefs`に`ui_scale: i32`（%）を追加する。`#[serde(default = "default_ui_scale")]`、既定は100。旧JSONはそのまま読める。
- 範囲は50〜200%。Rust側は他の数値項目（`column_opacity`等）と同様に値を不透明に永続化し、clamp/正規化はフロント（`lib/uiScale.ts`の純関数`normalizeUiScale`）で行う。壊れた値や範囲外の値でもズームが異常にならないよう、適用時に必ず正規化する。さらにストアは読み込み時・保存時にも正規化し、`app.ui.uiScale`を常に有効値（50〜200の整数）に保つ。設定画面のスライダーの初期値は、刻み（10）の最寄りに丸める。
- `tauri-specta`のバインディング（`frontend/src/bindings/tauri.gen.ts`）は`cargo test`で再生成する。手編集しない。

### 反映

- `store.svelte.ts`に`#applyUiScale(scale)`を追加し、`#applyFont`と同じ箇所（起動時の設定読み込みと`setUiPrefs`）から呼ぶ。中身は`getCurrentWebview().setZoom(normalizeUiScale(scale) / 100)`。ストアは直前に適用した倍率を保持し、同じ倍率ならスキップする（テーマなど無関係な保存のたびにIPCを往復させない）。失敗した場合は記録を戻し、次の保存で再試行する。
- モバイル（`isMobilePlatform`）では何もしない。`setZoom`の失敗（未対応環境など）は例外を握りつぶさずログへ出すが、アプリの動作は止めない。
- `src-tauri/capabilities/default.json`に`core:webview:allow-set-webview-zoom`を追加する。
- 起動時は保存値を読み込んだ後に適用するため、一瞬100%で描画されてから切り替わる。テーマ・フォントと同じ挙動であり、許容する。

### 設定UI

- `AppearanceSection.svelte`に「UIスケール(N%)」のスライダー（50〜200、10刻み）と「100%に戻す」ボタンを追加する。
- 他の項目と同じく「保存」で反映する。モバイルでは項目ごと非表示にする。
- ノート詳細や他画面の見た目への個別対応はしない。

### テスト

- Rust: `ui_scale`欠落の旧JSONが100で読めること、保存→読み込みのラウンドトリップ（`domain/ui.rs`の既存テスト群に追加）。`cargo test`の`generates_frontend_bindings`で`uiScale`が`camelCase`で出力されること。
- Vitest: `normalizeUiScale`（範囲外・NaN・非整数・未定義の扱い）。ストアの適用呼び出し（`setZoom`をモックして倍率と、モバイルで呼ばれないこと）。
- 実UI: `dbus-run-session`とXvfb（`WAYLAND_DISPLAY`もunset）の隔離環境で`cargo tauri dev`を起動し、WebKitGTK上で拡縮・再起動後の保持・ポップオーバー位置を目視確認する。この確認は省略しない。

### ドキュメント

- `docs/guide/user-guide.md`の外観設定の説明にUIスケールを追記する。

## 影響範囲

- `src-tauri/src/domain/ui.rs`（フィールド・既定値・テスト）
- `src-tauri/capabilities/default.json`
- `frontend/src/bindings/tauri.gen.ts`（生成）
- `frontend/src/lib/uiScale.ts`（新規）、`frontend/src/lib/store.svelte.ts`
- `frontend/src/ui/settings/AppearanceSection.svelte`
- `docs/guide/user-guide.md`

## 未確認事項

- WebKitGTK（Linux）・WKWebView（macOS）・WebView2（Windows）での`setZoom`の実挙動。実機で確認できるのはLinuxのみ。macOS/Windowsは公式docsの記述に依拠する。
