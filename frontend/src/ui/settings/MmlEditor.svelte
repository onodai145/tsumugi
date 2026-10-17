<script lang="ts">
  // MML 入力欄(Issue #500)。CodeMirror 6 を包み、色分け・エラー下線・カーソル移動を持つ。
  // 構文の検証はしない(親が parseMml の結果を errorPosition で渡す)。
  import { onMount } from "svelte";
  import { Compartment, EditorState } from "@codemirror/state";
  import { EditorView, keymap, placeholder } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { charPosToIndex, errorRange } from "../../lib/mmlPosition";
  import { errorField, mmlHighlight, mmlTheme, setErrorRange } from "./mmlEditorExtensions";

  let {
    value = $bindable(""),
    errorPosition = null,
    ariaLabel = "MML",
    describedBy,
    invalid = false,
  }: {
    value: string;
    /// parse_mml が返した、エラーの文字位置(Rust の添字)。null なら下線なし
    errorPosition?: number | null;
    ariaLabel?: string;
    describedBy?: string;
    invalid?: boolean;
  } = $props();

  let host = $state<HTMLDivElement>();
  let view = $state.raw<EditorView>();
  const a11y = new Compartment();

  function a11yExtension() {
    return EditorView.contentAttributes.of({
      role: "textbox",
      "aria-multiline": "true",
      "aria-label": ariaLabel,
      autocapitalize: "off",
      autocorrect: "off",
      spellcheck: "false",
      ...(describedBy ? { "aria-describedby": describedBy } : {}),
      ...(invalid ? { "aria-invalid": "true" } : {}),
    });
  }

  onMount(() => {
    if (!host) return;
    const v = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          placeholder("t120 o5 l8 cdefgab>c"),
          mmlHighlight,
          errorField,
          mmlTheme,
          a11y.of(a11yExtension()),
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) return;
            const text = update.state.doc.toString();
            if (text !== value) value = text;
          }),
        ],
      }),
    });
    view = v;
    return () => v.destroy();
  });

  // 外から value が変わったら、文書に反映する(エディタ側の変更で value が変わったときは何もしない)
  $effect(() => {
    const v = value;
    if (view && v !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
    }
  });

  $effect(() => {
    const pos = errorPosition;
    if (!view) return;
    const range = pos === null ? null : errorRange(view.state.doc.toString(), pos);
    // 編集で下線が自分で消えているなら、消すためだけの dispatch はしない(IME 変換中の更新を増やさない)
    if (range === null && view.state.field(errorField).size === 0) return;
    view.dispatch({ effects: setErrorRange.of(range) });
  });

  $effect(() => {
    if (!view) return;
    view.dispatch({ effects: a11y.reconfigure(a11yExtension()) });
  });

  /** Rust の文字位置にカーソルを置いて、見える位置までスクロールし、フォーカスする */
  export function focusAt(pos: number): void {
    if (!view) return;
    const anchor = charPosToIndex(view.state.doc.toString(), pos);
    view.dispatch({ selection: { anchor }, effects: EditorView.scrollIntoView(anchor, { y: "center" }) });
    view.focus();
  }
</script>

<div
  bind:this={host}
  class="cursor-text rounded-lg border border-border bg-muted text-foreground focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50"
  data-testid="mml-input"
></div>
