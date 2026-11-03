<script lang="ts">
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import SettingsImport from "./SettingsImport.svelte";
  import { commands, unwrap } from "../../lib/ipc";

  // インポート後にWebViewを読み込み直す。テストでは差し替える。
  let { reload = () => window.location.reload() }: { reload?: () => void } = $props();

  const filters = [{ name: "JSON", extensions: ["json"] }];

  let busy = $state(false);
  let err = $state<string | null>(null);
  let message = $state<string | null>(null);
  let importBusy = $state(false);

  function defaultFileName(): string {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `tsumugi-settings-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}.json`;
  }

  async function exportSettings() {
    err = null;
    message = null;
    busy = true;
    try {
      const path = await saveDialog({ defaultPath: defaultFileName(), filters });
      if (!path) return;
      await unwrap(commands.exportSettings(path));
      message = "書き出しました";
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h4 class="mb-2 mt-4 text-sm font-semibold text-muted-foreground">設定のバックアップ</h4>

<div class="mb-2 flex flex-wrap gap-2">
  <Button type="button" variant="outline" disabled={busy || importBusy} onclick={exportSettings}>エクスポート…</Button>
  <SettingsImport
    {reload}
    disabled={busy}
    bind:busy={importBusy}
    onerror={(m) => {
      err = m;
      if (m === null) message = null;
    }}
  />
</div>
<p class="mb-2 mt-0 text-xs text-muted-foreground">
  アカウント一覧・カラム構成・各種設定(カスタムCSSを含む)をファイルに書き出します。トークンは含みません。
  インポートすると現在の設定とカラムが置き換わり、取り込んだアカウントは再認証が必要になります。
  背景画像・動画とノートキャッシュは対象外です。
</p>
{#if message}
  <p class="mb-2 mt-0 text-sm text-muted-foreground" data-testid="settings-transfer-message">{message}</p>
{/if}
{#if err}
  <p class="mb-2 mt-0 text-sm text-destructive" data-testid="settings-transfer-error">{err}</p>
{/if}
