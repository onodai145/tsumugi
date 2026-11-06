<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";
  import { TriangleAlert } from "@lucide/svelte";

  // 設定のインポートで追加された、トークンなしのアカウントがあるときの案内(Issue #553)。
  // 再認証が済んで件数が0になれば自然に消えるので、閉じるボタンは持たない。
  let { onOpenAccounts }: { onOpenAccounts: () => void } = $props();
</script>

{#if app.needsLogin.size > 0}
  <div
    class="flex items-center gap-2 border-b border-border bg-muted px-3 py-2 text-sm"
    role="status"
    data-testid="reauth-banner"
  >
    <TriangleAlert size={16} class="flex-none text-[var(--warning)]" />
    <span class="min-w-0 flex-1">{app.needsLogin.size}件のアカウントは再認証が必要です</span>
    <Button variant="outline" size="xs" onclick={onOpenAccounts} data-testid="reauth-banner-open">
      設定のアカウントを開く
    </Button>
  </div>
{/if}
