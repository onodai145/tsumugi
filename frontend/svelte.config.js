/** @type {import("@sveltejs/vite-plugin-svelte").SvelteConfig} */
export default {
  vitePlugin: {
    // CSS を別の仮想モジュール(?svelte&type=style)として出さず JS に注入する。
    // vite-plugin-svelte の "failed to load virtual css module"
    // (WebView が JS を HTTP キャッシュから返し CSS meta が空になる問題)を回避。
    emitCss: false,
  },
};
