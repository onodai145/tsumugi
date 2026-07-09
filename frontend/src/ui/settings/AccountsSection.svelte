<script lang="ts">
  import { app } from "../../lib/store.svelte";

  let { onAddAccount }: { onAddAccount: () => void } = $props();

  let busyId = $state<string | null>(null);
  let confirmId = $state<string | null>(null);
  let err = $state<string | null>(null);

  async function remove(id: string) {
    err = null;
    busyId = id;
    try {
      await app.removeAccount(id);
      confirmId = null;
    } catch (e) {
      err = String(e);
    } finally {
      busyId = null;
    }
  }

  async function makeDefault(id: string) {
    err = null;
    try {
      await app.setUiPrefs({ ...app.ui, defaultAccountId: id });
    } catch (e) {
      err = String(e);
    }
  }
</script>

<h3 class="title">アカウント</h3>

{#if app.accounts.length === 0}
  <p class="hint">ログイン中のアカウントはありません。</p>
{:else}
  <ul class="list">
    {#each app.accounts as a (a.id)}
      <li class="item">
        {#if a.avatarUrl}
          <img class="avatar" src={a.avatarUrl} alt="" />
        {:else}
          <div class="avatar ph">{(a.displayName || a.username).charAt(0)}</div>
        {/if}
        <div class="meta">
          <div class="name">{a.displayName || a.username}{#if a.id === app.defaultAccountId()}<span class="default-badge">既定</span>{/if}</div>
          <div class="handle">@{a.username}@{a.host}</div>
        </div>
        {#if confirmId === a.id}
          <div class="confirm">
            <span>削除？</span>
            <button class="del" disabled={busyId === a.id} onclick={() => remove(a.id)}>
              {busyId === a.id ? "…" : "はい"}
            </button>
            <button class="ghost" onclick={() => (confirmId = null)}>いいえ</button>
          </div>
        {:else}
          {#if a.id !== app.defaultAccountId()}
            <button class="ghost" onclick={() => makeDefault(a.id)}>既定に設定</button>
          {/if}
          <button class="ghost" onclick={() => (confirmId = a.id)}>削除</button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<p class="hint">
  アカウントを削除すると、そのアカウントのカラム（タブ）も表示されなくなり、保存済みトークンは keyring から破棄されます。
</p>

<div class="actions">
  <button class="add" onclick={onAddAccount}>＋ アカウントを追加</button>
</div>
{#if err}<p class="err">{err}</p>{/if}

<style>
  .title {
    margin: 0 0 14px;
    font-size: 1rem;
    font-weight: 600;
  }
  .list {
    list-style: none;
    margin: 0 0 12px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }
  .avatar {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    flex: none;
    object-fit: cover;
  }
  .avatar.ph {
    display: grid;
    place-items: center;
    background: var(--surface-3);
    color: var(--text-dim);
    font-weight: 700;
  }
  .meta {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: 0.86rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .handle {
    font-size: 0.76rem;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .default-badge {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: var(--accent);
    font-size: 0.68rem;
    font-weight: 600;
  }
  .confirm {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.78rem;
    color: var(--text-dim);
    flex: none;
  }
  .ghost {
    padding: 5px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-1);
    color: var(--text);
    cursor: pointer;
    font-size: 0.78rem;
  }
  .del {
    padding: 5px 10px;
    border: none;
    border-radius: 6px;
    background: #ef4444;
    color: #fff;
    cursor: pointer;
    font-size: 0.78rem;
  }
  .del:disabled {
    opacity: 0.5;
  }
  .hint {
    font-size: 0.76rem;
    color: var(--text-dim);
    margin: 0 0 14px;
  }
  .actions {
    display: flex;
    justify-content: flex-start;
  }
  .add {
    padding: 7px 16px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: transparent;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
  }
</style>
