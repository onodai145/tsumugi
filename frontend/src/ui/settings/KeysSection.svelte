<script lang="ts">
  import { ACTIONS } from "../../lib/keymap";

  // 表示用にキー名を整形（例 "shift+r" → "Shift + R"）。
  function pretty(chord: string): string {
    return chord
      .split("+")
      .map((p) => {
        if (p === "ctrl") return "Ctrl";
        if (p === "meta") return "⌘";
        if (p === "alt") return "Alt";
        if (p === "shift") return "Shift";
        if (p === "space") return "Space";
        return p.length === 1 ? p.toUpperCase() : p;
      })
      .join(" + ");
  }

  // 追加で固定の操作（キーマップ外）
  const extra: { combo: string; desc: string }[] = [
    { combo: "Ctrl / ⌘ + Enter", desc: "投稿する（投稿バー・投稿フォーム）" },
    { combo: "Esc", desc: "モーダル／リアクションピッカーを閉じる" },
  ];
</script>

<h3 class="title">キー操作</h3>
<p class="hint">
  タイムライン上でキーを押すと、フォーカス中カラムの選択ノートを操作できます。ノートをクリックすると選択され、そのカラムにフォーカスが移ります。
</p>

<table>
  <tbody>
    {#each ACTIONS as a (a.action)}
      <tr>
        <td class="kbd"><kbd>{pretty(a.default)}</kbd></td>
        <td class="desc">{a.label}</td>
      </tr>
    {/each}
    {#each extra as e (e.combo)}
      <tr>
        <td class="kbd"><kbd>{e.combo}</kbd></td>
        <td class="desc">{e.desc}</td>
      </tr>
    {/each}
  </tbody>
</table>

<p class="hint">※ キーの割り当て変更（カスタマイズ）は今後の対応予定です。</p>

<style>
  .title {
    margin: 0 0 10px;
    font-size: 1rem;
    font-weight: 600;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.84rem;
    margin: 8px 0;
  }
  td {
    padding: 5px 6px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  .kbd {
    width: 38%;
    white-space: nowrap;
  }
  kbd {
    display: inline-block;
    padding: 2px 7px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--surface-2);
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
  }
  .desc {
    color: var(--text);
  }
  .hint {
    font-size: 0.76rem;
    color: var(--text-dim);
    margin: 8px 0;
  }
</style>
