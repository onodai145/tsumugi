<script lang="ts">
  import SaveBar from "./SaveBar.svelte";
  import { app } from "../../lib/store.svelte";

  let hapticsEnabled = $state(app.ui.hapticsEnabled ?? true);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // 編集した項目だけを渡す。保存は直列化され、最新の app.ui にマージされる(Issue #614)。
      await app.updateUiPrefs({
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
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  ノート投稿時とリアクション付与時に端末を振動させます。触覚入りの ogg を通知音にしたときの振動(Android)も、この設定に従います。
</p>

<SaveBar {busy} {saved} {err} onsave={save} />
