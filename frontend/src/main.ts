import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { commands } from './lib/ipc'

// WebView既定の右クリックメニュー(「戻る」「ページの検証」等、デスクトップアプリには
// そぐわないブラウザ由来の項目)を隠す(Issue #7)。ただし入力欄(投稿欄等)では
// 貼り付け/コピー/切り取りに右クリックメニューを使いたいので、そこだけは残す。
window.addEventListener('contextmenu', (e) => {
  const tag = (e.target as HTMLElement | null)?.tagName
  if (tag !== 'TEXTAREA' && tag !== 'INPUT') e.preventDefault()
})

// Ctrl/Cmd+A の既定「全選択」は user-select:none を無視して文書全体(ヘッダーや
// ボタンラベルまで)を選択してしまう(Issue #67)。入力欄以外では発火させない。
window.addEventListener('keydown', (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'a') return
  const tag = (e.target as HTMLElement | null)?.tagName
  if (tag !== 'TEXTAREA' && tag !== 'INPUT') e.preventDefault()
})

// 右クリックの「検証」メニューを隠した代わりに、開発ビルド限定でF12からDevToolsを開けるようにする。
// Rust側 open_devtools コマンドもデバッグビルド限定(リリースビルドでは無害なno-op)。
if (import.meta.env.DEV) {
  window.addEventListener('keydown', (e) => {
    if (e.key === 'F12') commands.openDevtools()
  })
}

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
