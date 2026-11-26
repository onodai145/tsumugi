// 型情報を使うルールは Promise の取りこぼし(no-floating-promises / no-misused-promises)だけにする(Issue #707)。
import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import ts from "typescript-eslint";
import svelteConfig from "./svelte.config.js";

export default defineConfig(
  globalIgnores(["dist/", "coverage/", "public/", "src/bindings/"]),
  js.configs.recommended,
  ts.configs.recommended,
  svelte.configs.recommended,
  { languageOptions: { globals: { ...globals.browser, ...globals.node } } },
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: { parserOptions: { parser: ts.parser, svelteConfig } },
  },
  {
    files: ["**/*.ts", "**/*.svelte"],
    languageOptions: {
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname, extraFileExtensions: [".svelte"] },
    },
    rules: {
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
    },
  },
  {
    // `trigger;` や `app.now;` は、$effect / $derived に依存を追跡させるための式。
    files: ["**/*.svelte", "**/*.svelte.ts"],
    rules: { "@typescript-eslint/no-unused-expressions": "off" },
  },
  {
    rules: {
      // キーを後から足すと、重複があれば実行時エラーになる。
      "svelte/require-each-key": "off",
      // 誤検知する。指摘されたコメントを消すと svelte-check の警告が増える。
      "svelte/no-unused-svelte-ignore": "off",
      // `{"a\nb"}` は属性に直すと改行が失われる。
      "svelte/no-useless-mustaches": ["error", { ignoreStringEscape: true }],
      // 全角空白を意図して正規表現に書いている。
      "no-irregular-whitespace": ["error", { skipRegExps: true }],
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
    },
  },
);
