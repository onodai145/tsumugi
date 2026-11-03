<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import { commands, unwrap, type ImportPreview } from "../../lib/ipc";

  // 設定のインポート一式(ボタン・ファイル選択・確認ダイアログ・取り込み・リロード)。
  // 設定タブ(SettingsTransfer)と、アカウント0件の新規インストール時のアカウント追加画面で共有する。
  let {
    // インポート後にWebViewを読み込み直す。テストでは差し替える。
    reload = () => window.location.reload(),
    // 親側の都合で押せなくしたいとき(エクスポート中など)。
    disabled = false,
    // 新しい試行の開始時にnull、失敗時にエラー文字列で呼ばれる。
    onerror,
    // 処理中かどうか。親が他のボタンの無効化に使う。
    busy = $bindable(false),
  }: {
    reload?: () => void;
    disabled?: boolean;
    onerror?: (message: string | null) => void;
    busy?: boolean;
  } = $props();

  const filters = [{ name: "JSON", extensions: ["json"] }];

  let pendingPath = $state<string | null>(null);
  let preview = $state<ImportPreview | null>(null);

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

  async function pickImportFile() {
    onerror?.(null);
    busy = true;
    try {
      const picked = await openDialog({ multiple: false, directory: false, filters });
      if (!picked || Array.isArray(picked)) return;
      preview = await unwrap(commands.inspectSettingsFile(picked));
      pendingPath = picked;
    } catch (e) {
      onerror?.(String(e));
    } finally {
      busy = false;
    }
  }

  function cancelImport() {
    pendingPath = null;
    preview = null;
  }

  async function confirmImport() {
    // 二重確定を避けるため、importSettingsを呼ぶ前に同期的に保留中のパスを消す。
    const path = pendingPath;
    cancelImport();
    if (!path) return;
    onerror?.(null);
    busy = true;
    try {
      await unwrap(commands.importSettings(path));
      reload();
    } catch (e) {
      onerror?.(String(e));
    } finally {
      busy = false;
    }
  }
</script>

<Button type="button" variant="outline" disabled={busy || disabled} onclick={pickImportFile}>インポート…</Button>

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
