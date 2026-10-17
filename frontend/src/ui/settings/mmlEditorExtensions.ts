// MML エディタの CodeMirror 拡張(Issue #500)。色分け・エラー下線・見た目。
// 色は既存のテーマトークンだけを使う(カスタムテーマの対応を増やさない)。色だけでなく太字でも区別する。
import { RangeSetBuilder, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { lexMml, type MmlTokenKind } from "../../lib/mmlLexer";

const MARKS: Record<MmlTokenKind, Decoration | null> = {
  note: null,
  unknown: null,
  rest: Decoration.mark({ class: "mml-rest" }),
  command: Decoration.mark({ class: "mml-command" }),
  timbre: Decoration.mark({ class: "mml-timbre" }),
  number: Decoration.mark({ class: "mml-number" }),
  repeat: Decoration.mark({ class: "mml-repeat" }),
  separator: Decoration.mark({ class: "mml-separator" }),
};

function buildDecorations(doc: string): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const token of lexMml(doc)) {
    const mark = MARKS[token.kind];
    if (mark) builder.add(token.from, token.to, mark);
  }
  return builder.finish();
}

/** 文書全体を字句解析して色分けする。上限が 5000 文字なので、編集のたびに作り直して足りる */
export const mmlHighlight: Extension = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view.state.doc.toString());
    }
    update(update: ViewUpdate) {
      if (update.docChanged) this.decorations = buildDecorations(update.state.doc.toString());
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

const errorMark = Decoration.mark({ class: "mml-error" });

/** エラー下線の範囲を設定する。null で消す */
export const setErrorRange = StateEffect.define<{ from: number; to: number } | null>();

export const errorField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(decorations, tr) {
    // 編集すると位置がずれて、古い下線はもう正しくない。親からの消去を待たず、ここで消す
    // (IME 変換中に、変換とは別の transaction を増やさないため)。同じ transaction の設定は下で勝つ
    let next = tr.docChanged ? Decoration.none : decorations;
    for (const effect of tr.effects) {
      if (!effect.is(setErrorRange)) continue;
      const range = effect.value;
      if (!range) {
        next = Decoration.none;
        continue;
      }
      const from = Math.max(0, range.from);
      const to = Math.min(range.to, tr.state.doc.length);
      next = from < to ? Decoration.set([errorMark.range(from, to)]) : Decoration.none;
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** 枠(角丸・枠線・フォーカスリング)は MmlEditor.svelte のホスト要素が持つ。ここは中身だけ */
export const mmlTheme: Extension = EditorView.theme({
  "&": { fontSize: "0.75rem", color: "var(--text)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace)",
    lineHeight: "1.5",
  },
  ".cm-content": { padding: "0.5rem 0.625rem", minHeight: "calc(4.5em + 1rem)", caretColor: "var(--text)" },
  ".cm-line": { padding: "0" },
  ".cm-placeholder": { color: "var(--text-dim)" },
  ".mml-rest": { color: "var(--text-dim)" },
  ".mml-command": { color: "var(--accent)" },
  ".mml-timbre": { color: "color-mix(in srgb, var(--accent) 50%, var(--text))", fontWeight: "700" },
  ".mml-number": { color: "color-mix(in srgb, var(--accent) 35%, var(--text))" },
  ".mml-repeat": { fontWeight: "700" },
  ".mml-separator": { color: "var(--text-dim)", fontWeight: "700" },
  ".mml-error": { textDecoration: "underline wavy var(--danger)", textUnderlineOffset: "2px" },
});
