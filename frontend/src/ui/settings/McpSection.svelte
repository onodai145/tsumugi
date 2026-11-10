<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../../lib/store.svelte";
  import { commands, type McpStatus } from "../../lib/ipc";
  import { Button } from "$lib/components/ui/button";

  let status = $state<McpStatus | null>(null);
  let portInput = $state(49731);
  let token = $state<string | null>(null);
  let tokenVisible = $state(false);
  let confirmRegenerate = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let copied = $state<string | null>(null);

  const endpoint = $derived(`http://127.0.0.1:${status?.port ?? 49731}/mcp`);
  const stateLabel = $derived.by(() => {
    const s = status?.state;
    if (!s) return "";
    if (s.type === "running") return "稼働中";
    if (s.type === "error") return `エラー: ${s.message}`;
    return "停止中";
  });

  function apply(s: McpStatus) {
    status = s;
    portInput = s.port;
  }

  onMount(async () => {
    const s = await run(() => commands.getMcpStatus());
    if (s) apply(s);
  });

  async function run<T>(f: () => Promise<{ status: "ok"; data: T } | { status: "error"; error: unknown }>) {
    err = null;
    busy = true;
    try {
      const r = await f();
      if (r.status === "error") {
        err = String(r.error);
        return null;
      }
      return r.data;
    } catch (e) {
      err = String(e);
      return null;
    } finally {
      busy = false;
    }
  }

  async function toggleEnabled(e: Event) {
    // await の後は e.currentTarget が null になるので、先に要素を持っておく。
    const el = e.currentTarget as HTMLInputElement;
    const s = await run(() => commands.setMcpEnabled(el.checked));
    if (s) apply(s);
    else el.checked = status?.enabled ?? false; // 失敗したら、表示を保存済みの状態に戻す
  }

  async function commitPort() {
    if (!status || portInput === status.port) return;
    const s = await run(() => commands.setMcpPort(portInput));
    if (s) apply(s);
    else portInput = status.port;
  }

  async function toggleAccount(e: Event, id: string) {
    const el = e.currentTarget as HTMLInputElement;
    const s = await run(() => commands.setMcpAccountExposed(id, el.checked));
    if (s) apply(s);
    else el.checked = status?.exposedAccountIds.includes(id) ?? false;
  }

  async function toggleToken() {
    if (tokenVisible) {
      tokenVisible = false;
      return;
    }
    if (token === null) {
      const t = await run(() => commands.getMcpToken());
      if (t === null) return;
      token = t;
    }
    tokenVisible = true;
  }

  async function regenerate() {
    const t = await run(() => commands.regenerateMcpToken());
    confirmRegenerate = false;
    if (t === null) return;
    token = t;
    tokenVisible = true;
  }

  async function copy(kind: string, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = kind;
      setTimeout(() => (copied = copied === kind ? null : copied), 1500);
    } catch {
      err = "コピーできませんでした";
    }
  }

  async function ensureToken(): Promise<string | null> {
    if (token === null) {
      const t = await run(() => commands.getMcpToken());
      if (t === null) return null;
      token = t;
    }
    return token;
  }

  async function copyToken() {
    const t = await ensureToken();
    if (t !== null) await copy("token", t);
  }

  async function copySnippet() {
    const t = await ensureToken();
    if (t === null) return;
    await copy("snippet", `claude mcp add --transport http tsumugi ${endpoint} --header "Authorization: Bearer ${t}"`);
  }
</script>

<h3 class="mb-2 mt-0 text-base font-semibold">MCPサーバー</h3>
<p class="mb-3.5 mt-0 text-sm text-muted-foreground">
  AIアシスタント(MCPクライアント)から、このアプリがキャッシュしたノートを検索できるようにします。読み取り専用で、この端末の中(127.0.0.1)だけで待ち受けます。
</p>
<p data-testid="mcp-warning" class="mb-3.5 mt-0 rounded-lg border border-border bg-muted px-2.5 py-2 text-sm">
  公開を許可したアカウントの、キャッシュ済みの「公開」「ホーム」のノートが、アプリの外のAIへ渡ります。返るノートの本文は第三者が書いた入力で、AIがそこに書かれた指示に従う恐れがあります。信頼できるクライアントだけに設定してください。
</p>

{#if status}
  <label class="mb-2 flex items-center gap-2 text-sm">
    <input
      type="checkbox"
      data-testid="mcp-enabled"
      checked={status.enabled}
      disabled={busy}
      onchange={toggleEnabled}
    />
    MCPサーバーを有効にする
  </label>
  <p data-testid="mcp-state" class="mb-3 mt-0 text-xs text-muted-foreground">{stateLabel}</p>
  {#if err}
    <p data-testid="mcp-error" class="mb-3 mt-0 text-sm text-destructive">{err}</p>
  {/if}

  <div class="mb-3.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">公開するアカウント(初期状態はすべてオフ)</span>
    {#each app.accounts as a (a.id)}
      <label class="flex items-center gap-2">
        <input
          type="checkbox"
          data-testid={`mcp-account-${a.id}`}
          checked={status.exposedAccountIds.includes(a.id)}
          disabled={busy}
          onchange={(e) => toggleAccount(e, a.id)}
        />
        {a.displayName}(@{a.username}@{a.host})
      </label>
    {/each}
  </div>

  <label class="mb-3.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">ポート(1024〜65535)</span>
    <input
      class="w-32 rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
      type="number"
      min="1024"
      max="65535"
      data-testid="mcp-port"
      bind:value={portInput}
      onchange={commitPort}
    />
  </label>

  <div class="mb-3.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">エンドポイント</span>
    <div class="flex items-center gap-2">
      <code data-testid="mcp-endpoint" class="rounded-lg border border-border bg-muted px-2.5 py-2">{endpoint}</code>
      <Button variant="outline" size="sm" onclick={() => copy("endpoint", endpoint)}>
        {copied === "endpoint" ? "コピーしました" : "コピー"}
      </Button>
    </div>
  </div>

  <div class="mb-3.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">トークン(クライアントの設定に貼り付けます。他人に見せないでください)</span>
    <div class="flex flex-wrap items-center gap-2">
      <code data-testid="mcp-token" class="rounded-lg border border-border bg-muted px-2.5 py-2">
        {tokenVisible && token !== null ? token : "••••••••••••••••••••"}
      </code>
      <Button variant="outline" size="sm" data-testid="mcp-token-toggle" onclick={toggleToken}>
        {tokenVisible ? "隠す" : "表示"}
      </Button>
      <Button variant="outline" size="sm" data-testid="mcp-token-copy" onclick={copyToken}>
        {copied === "token" ? "コピーしました" : "コピー"}
      </Button>
      {#if !confirmRegenerate}
        <Button variant="outline" size="sm" data-testid="mcp-regenerate" onclick={() => (confirmRegenerate = true)}>
          再生成
        </Button>
      {:else}
        <span class="text-xs text-muted-foreground">古いトークンは直ちに使えなくなります。</span>
        <Button variant="destructive" size="sm" data-testid="mcp-regenerate-confirm" onclick={regenerate}>
          再生成する
        </Button>
        <Button variant="outline" size="sm" onclick={() => (confirmRegenerate = false)}>キャンセル</Button>
      {/if}
    </div>
  </div>

  <div class="mb-3.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">Claude Code に登録するコマンド</span>
    <div>
      <Button variant="outline" size="sm" data-testid="mcp-snippet-copy" onclick={copySnippet}>
        {copied === "snippet" ? "コピーしました" : "コマンドをコピー"}
      </Button>
    </div>
  </div>
{/if}
