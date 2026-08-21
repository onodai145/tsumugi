// shiki によるコードブロックのシンタックスハイライト（Issue #118）。
// ハイライタはモジュールスカラーのシングルトンとして遅延生成する
// （マルチカラムで多数のノートが同時描画されるため、1インスタンスを使い回す）。
import type { HighlighterGeneric } from "shiki/core";
import { createHighlighter, createCssVariablesTheme } from "shiki";
import { createJavaScriptRegexEngine } from "@shikijs/engine-javascript";
import type { CustomSyntaxTheme } from "../bindings/tauri.gen";

export const BUNDLED_LANGS = [
  "typescript",
  "tsx",
  "javascript",
  "jsx",
  "rust",
  "bash",
  "json",
  "yaml",
  "toml",
  "python",
  "go",
  "sql",
  "markdown",
  "html",
  "css",
  "ts",
  "js",
  "py",
  "rs",
  "sh",
  "md",
  "yml",
] as const;

const AUTO_LIGHT_THEME = "github-light";
const AUTO_DARK_THEME = "github-dark";
// shikiに "css-variables" という同梱テーマは存在しない（shiki 4.3.1で確認済み、Task 1参照）。
// createCssVariablesTheme() で --shiki-token-* を出力するテーマオブジェクトを1つ作り、
// codeToHtml() の theme に登録名ではなく直接渡す（値はユーザーごとに変わらず、
// 実際の色は lib/theme.ts が <html> に設定する --shiki-token-* 側で決まるため使い回せる）。
const CSS_VARIABLES_THEME = createCssVariablesTheme({ name: "css-variables", variablePrefix: "--shiki-" });

let highlighterPromise: Promise<HighlighterGeneric<any, any>> | null = null;

function getHighlighter() {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [AUTO_LIGHT_THEME, AUTO_DARK_THEME],
      langs: [],
      engine: createJavaScriptRegexEngine(),
    });
  }
  return highlighterPromise;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function plainHtml(code: string): string {
  return `<pre class="shiki-plain"><code>${escapeHtml(code)}</code></pre>`;
}

/// customSyntaxThemes から id を引いて --shiki-token-* を <html> に反映するのは
/// lib/theme.ts の applySyntaxTheme() の責務。ここではハイライト結果HTMLの生成のみ行う。
export async function highlightCode(
  code: string,
  lang: string | null,
  themeSelection: string,
  customSyntaxThemes: CustomSyntaxTheme[],
): Promise<string> {
  const normalizedLang = (lang ?? "").toLowerCase();
  const isBundled = BUNDLED_LANGS.includes(normalizedLang as (typeof BUNDLED_LANGS)[number]);
  // 言語未指定・非対応言語は shiki 組み込みの特殊言語 "text"（isPlainLang、文法ロード不要）に
  // フォールバックする（Issue #229）。構文強調はされないが、独自の plainHtml() と違い
  // shiki の codeToHtml() を通るためテーマの背景色等は他のコードブロックと統一される。
  const effectiveLang = isBundled ? normalizedLang : "text";
  try {
    const highlighter = await getHighlighter();
    if (isBundled) {
      const loadedLangs = highlighter.getLoadedLanguages();
      if (!loadedLangs.includes(normalizedLang)) {
        await highlighter.loadLanguage(normalizedLang as any);
      }
    }

    if (themeSelection === "auto") {
      return highlighter.codeToHtml(code, {
        lang: effectiveLang,
        themes: { light: AUTO_LIGHT_THEME, dark: AUTO_DARK_THEME },
        defaultColor: false,
      });
    }
    if (themeSelection.startsWith("custom:")) {
      return highlighter.codeToHtml(code, { lang: effectiveLang, theme: CSS_VARIABLES_THEME });
    }
    if (themeSelection.startsWith("shiki:")) {
      const themeId = themeSelection.slice("shiki:".length);
      const loadedThemes = highlighter.getLoadedThemes();
      if (!loadedThemes.includes(themeId)) {
        await highlighter.loadTheme(themeId as any);
      }
      return highlighter.codeToHtml(code, { lang: effectiveLang, theme: themeId });
    }
    // 未知の themeSelection 値（旧データ等）は auto 相当にフォールバック
    return highlighter.codeToHtml(code, {
      lang: effectiveLang,
      themes: { light: AUTO_LIGHT_THEME, dark: AUTO_DARK_THEME },
      defaultColor: false,
    });
  } catch {
    return plainHtml(code);
  }
}
