<script lang="ts">
  import { commands } from "../../bindings/tauri.gen";
  import type { CacheBackendConfig } from "../../bindings/tauri.gen";
  import { Button } from "$lib/components/ui/button";

  let mode = $state<CacheBackendConfig["type"]>("sqlite");
  let host = $state("");
  let port = $state(5432);
  let database = $state("");
  let user = $state("");
  let password = $state("");
  let loading = $state(true);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  $effect(() => {
    void commands.getCacheBackend().then((r) => {
      loading = false;
      if (r.status !== "ok") return;
      mode = r.data.type;
      if (r.data.type === "postgres") {
        host = r.data.host;
        port = r.data.port;
        database = r.data.database;
        user = r.data.user;
      }
    });
  });

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      const config: CacheBackendConfig =
        mode === "sqlite" ? { type: "sqlite" } : { type: "postgres", host, port, database, user };
      const r = await commands.setCacheBackend(config, mode === "postgres" ? password : null);
      if (r.status !== "ok") {
        err = r.error;
        return;
      }
      password = "";
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-2 mt-0 text-base font-semibold">ノートキャッシュのバックエンド</h3>
<p class="mb-3.5 mt-0 text-sm text-muted-foreground">
  切り替えは即座に反映されます(再起動不要)。接続に失敗した場合、切替前のバックエンドのまま維持されます。
</p>

{#if !loading}
  <label class="mb-2 flex items-center gap-2 text-sm">
    <input type="radio" bind:group={mode} value="sqlite" />
    SQLite(ローカル、既定)
  </label>
  <label class="mb-2 flex items-center gap-2 text-sm">
    <input type="radio" bind:group={mode} value="postgres" />
    PostgreSQL
  </label>

  {#if mode === "postgres"}
    <div class="mt-2 flex flex-col gap-2.5">
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">ホスト</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="text" bind:value={host} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">ポート</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="1" max="65535" bind:value={port} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">データベース名</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="text" bind:value={database} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">ユーザー名</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="text" bind:value={user} />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">パスワード</span>
        <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="password" bind:value={password} />
      </label>
    </div>
  {/if}
{/if}

<div class="mt-3 flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">切り替えました</span>{/if}
  <Button type="button" disabled={busy || loading} onclick={save}>{busy ? "接続確認中…" : "保存して切り替え"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive" role="alert">接続に失敗しました: {err}</p>{/if}
