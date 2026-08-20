// MFM検索構文($[search]相当、`<query>\n検索`)をクリックしたときに開くURLを組み立てる。
// UiPrefs.searchEngineUrl の `{query}` をURLエンコード済みクエリで置換する（Issue #217）。
// テンプレートが空/プレースホルダを含まない(壊れた保存値・移行前の値)場合は既定のGoogle検索に
// フォールバックする。

export const DEFAULT_SEARCH_ENGINE_URL = "https://www.google.com/search?q={query}";

export const SEARCH_ENGINE_PRESETS: { label: string; url: string }[] = [
  { label: "Google", url: "https://www.google.com/search?q={query}" },
  { label: "Bing", url: "https://www.bing.com/search?q={query}" },
  { label: "DuckDuckGo", url: "https://duckduckgo.com/?q={query}" },
  { label: "Yahoo! JAPAN", url: "https://search.yahoo.co.jp/search?p={query}" },
];

export function buildSearchUrl(template: string | undefined, query: string): string {
  const t = template && template.includes("{query}") ? template : DEFAULT_SEARCH_ENGINE_URL;
  return t.replace("{query}", encodeURIComponent(query));
}
