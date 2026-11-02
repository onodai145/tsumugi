<script lang="ts">
  import SaveBar from "./SaveBar.svelte";
  import { app } from "../../lib/store.svelte";

  let enableFileLogging = $state(app.ui.enableFileLogging ?? false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // このセクションが編集しないフィールド(データ・外観等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        enableFileLogging,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">開発者オプション</h3>

<label class="mb-2 flex items-center gap-2 text-sm"><input type="checkbox" bind:checked={enableFileLogging} /> 動作ログをファイルに残す(デバッグ用)</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  WebSocket再接続やpingタイムアウトなどの内部ログを、アプリのログディレクトリにファイルとして
  永続化します。通知が来るタイミングがおかしい等の不具合調査用で、既定はOFFです。
  切り替えは次回起動から反映されます。
</p>

<SaveBar {busy} {saved} {err} onsave={save} />
