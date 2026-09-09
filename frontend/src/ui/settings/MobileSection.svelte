<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";

  let hapticsEnabled = $state(app.ui.hapticsEnabled ?? true);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // このセクションが編集しないフィールド(レイアウト・外観等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        hapticsEnabled,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">モバイル</h3>

<label class="mb-2 flex items-center gap-2 text-sm"
  ><input type="checkbox" bind:checked={hapticsEnabled} /> ハプティクス(振動)を有効にする</label
>
<p class="mb-4 mt-0 text-xs text-muted-foreground">ノート投稿時とリアクション付与時に端末を振動させます。</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
