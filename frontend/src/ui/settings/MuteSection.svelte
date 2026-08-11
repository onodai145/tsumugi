<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";

  // 1行1エントリのテキストで編集
  let words = $state(app.mute.ngWords.join("\n"));
  let users = $state(app.mute.ngUsers.join("\n"));
  let instances = $state(app.mute.ngInstances.join("\n"));
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  const lines = (s: string) =>
    s
      .split("\n")
      .map((x) => x.trim())
      .filter(Boolean);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      await app.setMute({
        ngWords: lines(words),
        ngUsers: lines(users),
        ngInstances: lines(instances),
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-2 mt-0 text-base font-semibold">NG(ミュート)</h3>
<p class="mb-3.5 mt-0 text-[0.78rem] text-muted-foreground">1行につき1件。以降に受信するノートに適用され、表示中の該当ノートも消えます。</p>

<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">NGワード(本文/CWに含むと非表示・部分一致)</span>
  <textarea class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" rows="3" placeholder={"ネタバレ\nspoiler"} bind:value={words}></textarea>
</label>
<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">NGユーザ(@user@host。@は省略可)</span>
  <textarea class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" rows="2" placeholder={"@spammer@example.com"} bind:value={users}></textarea>
</label>
<label class="mb-2.5 flex flex-col gap-1 text-[0.82rem]">
  <span class="text-muted-foreground">NGインスタンス(host)</span>
  <textarea class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" rows="2" placeholder={"spam.example"} bind:value={instances}></textarea>
</label>

<div class="mt-1 flex items-center justify-end gap-3">
  {#if saved}<span class="text-[0.8rem] text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-[0.82rem] text-destructive">{err}</p>{/if}
