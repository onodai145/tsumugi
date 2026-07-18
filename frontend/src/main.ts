import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

// WebView既定の右クリックメニュー(「戻る」「ページの検証」等、デスクトップアプリには
// そぐわないブラウザ由来の項目)を隠す(Issue #7)。
window.addEventListener('contextmenu', (e) => e.preventDefault())

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
