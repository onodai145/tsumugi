<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import { commands, unwrap, type ImportPreview } from "../../lib/ipc";

  // インポート後にWebViewを読み込み直す。テストでは差し替える。
  let { reload = () => window.location.reload() }: { reload?: () => void } = $props();

  const filters = [{ name: "JSON", extensions: ["json"] }];

  let busy = $state(false);
  let err = $state<string | null>(null);
  let message = $state<string | null>(null);
  let pendingPath = $state<string | null>(null);
  let preview = $state<ImportPreview | null>(null);

  function defaultFileName(): string {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `tsumugi-settings-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}.json`;
  }

  function previewMessage(p: ImportPreview): string {
    const lines = [`カラム${p.columnCount}件・アカウント${p.accountCount}件を読み込みます。`];
    if (p.accountsNeedingLogin > 0) {
      lines.push(`うち${p.accountsNeedingLogin}件のアカウントは、取り込み後に再認証が必要です。`);
    }
    if (p.hasCustomCss) lines.push("カスタムCSSを含みます。");
    if (p.hasCustomThemes) lines.push("カスタムテーマを含みます。");
    if (p.hasCustomEndpoints) {
      lines.push("検索エンジンまたはURLプレビューの送信先URLが、初期値から変更されています。");
    }
    lines.push("現在の設定とカラムは置き換えられます。");
    return lines.join("\n");
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

  async function pickImportFile() {
    err = null;
    message = null;
    busy = true;
    try {
      const picked = await openDialog({ multiple: false, directory: false, filters });
      if (!picked || Array.isArray(picked)) return;
      preview = await unwrap(commands.inspectSettingsFile(picked));
      pendingPath = picked;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  function cancelImport() {
    pendingPath = null;
    preview = null;
  }

  async function confirmImport() {
    const path = pendingPath;
    cancelImport();
    if (!path) return;
    err = null;
    busy = true;
    try {
      await unwrap(commands.importSettings(path));
      reload();
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h4 class="mb-2 mt-4 text-sm font-semibold text-muted-foreground">設定のバックアップ</h4>

<div class="mb-2 flex flex-wrap gap-2">
  <Button type="button" variant="outline" disabled={busy} onclick={exportSettings}>エクスポート…</Button>
  <Button type="button" variant="outline" disabled={busy} onclick={pickImportFile}>インポート…</Button>
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

{#if preview && pendingPath}
  <ConfirmDialog
    title="設定をインポート"
    message={previewMessage(preview)}
    confirmLabel="インポート"
    danger
    onConfirm={confirmImport}
    onCancel={cancelImport}
  />
{/if}
