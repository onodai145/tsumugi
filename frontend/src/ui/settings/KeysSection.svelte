<script lang="ts">
  // 現状実装済みのキー操作の一覧（参照専用）。カスタマイズは今後対応予定。
  const groups: { label: string; keys: { combo: string; desc: string }[] }[] = [
    {
      label: "投稿",
      keys: [{ combo: "Ctrl / ⌘ + Enter", desc: "投稿する（投稿バー・投稿フォーム）" }],
    },
    {
      label: "ダイアログ",
      keys: [
        { combo: "Esc", desc: "モーダル（設定・投稿・カラム追加）を閉じる" },
        { combo: "Enter", desc: "アカウント追加のホスト入力を確定" },
      ],
    },
  ];
</script>

<h3 class="title">キー操作</h3>

{#each groups as g (g.label)}
  <div class="grp">
    <div class="grp-label">{g.label}</div>
    <table>
      <tbody>
        {#each g.keys as k (k.combo)}
          <tr>
            <td class="kbd"><kbd>{k.combo}</kbd></td>
            <td class="desc">{k.desc}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/each}

<p class="hint">
  ※ タイムラインの j / k 移動やキーのカスタマイズは今後の対応予定です。
</p>

<style>
  .title {
    margin: 0 0 14px;
    font-size: 1rem;
    font-weight: 600;
  }
  .grp {
    margin-bottom: 14px;
  }
  .grp-label {
    font-size: 0.78rem;
    color: var(--text-dim);
    margin-bottom: 6px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.84rem;
  }
  td {
    padding: 5px 6px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  .kbd {
    width: 40%;
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
    margin: 8px 0 0;
  }
</style>
