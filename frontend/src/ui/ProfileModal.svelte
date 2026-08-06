<script lang="ts">
  import type { Note } from "../bindings/tauri.gen";
  import type { ProfileTarget } from "../lib/profileModal.svelte";
  import { app } from "../lib/store.svelte";
  import { acct, displayName } from "../lib/userDisplay";
  import { proxiedEmojiMap } from "../lib/emoji";
  import Modal from "./Modal.svelte";
  import Mfm from "../render/Mfm.svelte";
  import NoteCard from "./NoteCard.svelte";
  import FollowListModal from "./FollowListModal.svelte";

  let { target, accountId, onclose }: { target: ProfileTarget; accountId: string; onclose: () => void } =
    $props();

  // NoteCard.svelte と同じパターン: リモートユーザーのカスタム絵文字はaccountの接続先
  // インスタンス経由のプロキシURLで解決する必要がある（生URLのままだと表示されないことがある）。
  const instanceHost = $derived(app.accounts.find((a) => a.id === accountId)?.host);

  type ProfileState =
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "ready"; profile: Awaited<ReturnType<typeof app.getUserProfile>> };

  let profileState = $state<ProfileState>({ status: "loading" });
  let notes = $state<Note[]>([]);
  let notesBusy = $state(false);
  let notesDone = $state(false);
  let notesErr = $state<string | null>(null);
  let followBusy = $state(false);
  let followErr = $state<string | null>(null);
  let followListKind = $state<"followers" | "following" | null>(null);

  // target が変わるたびに load() が再実行されるが、直前のユーザー向けに投げた
  // getUserProfile/getUserNotes が新しい load() 開始後に解決するレースがあり得る
  // (例: FollowListModal 経由で target が連続して切り替わる場合)。それが reset 後の
  // profileState/notes を汚染しないよう、呼び出しごとに世代番号を発行し、自分の世代が
  // 最新でなければ結果を無視する。
  let requestGen = 0;

  async function resolveUserId(): Promise<string> {
    if ("userId" in target) return target.userId;
    const acctStr = target.host ? `${target.username}@${target.host}` : target.username;
    const u = await app.resolveUser(accountId, acctStr);
    return u.id;
  }

  async function load() {
    // target が変わって同じコンポーネントインスタンスが再利用されるケースがあるため、
    // 前のユーザーの状態を必ず捨てる。
    const myGen = ++requestGen;
    profileState = { status: "loading" };
    notes = [];
    notesDone = false;
    notesBusy = false;
    notesErr = null;
    followErr = null;
    followListKind = null;
    try {
      const userId = await resolveUserId();
      if (myGen !== requestGen) return;
      const profile = await app.getUserProfile(accountId, userId);
      if (myGen !== requestGen) return;
      profileState = { status: "ready", profile };
      void loadMoreNotes(profile.user.id, myGen);
    } catch (e) {
      if (myGen !== requestGen) return;
      profileState = { status: "error", message: String(e) };
    }
  }

  async function loadMoreNotes(userId: string, myGen: number = requestGen) {
    if (notesBusy || notesDone) return;
    notesBusy = true;
    notesErr = null;
    try {
      const untilId = notes.length > 0 ? notes[notes.length - 1].id : undefined;
      const page = await app.getUserNotes(accountId, userId, untilId);
      if (myGen !== requestGen) return; // 世代遅れの応答は profileState/notes に反映しない
      if (page.length === 0) notesDone = true;
      notes = [...notes, ...page];
    } catch (e) {
      if (myGen !== requestGen) return;
      notesErr = String(e);
    } finally {
      if (myGen === requestGen) notesBusy = false;
    }
  }

  async function toggleFollow() {
    if (profileState.status !== "ready" || profileState.profile.isFollowing === null) return;
    followBusy = true;
    followErr = null;
    const wasFollowing = profileState.profile.isFollowing;
    profileState.profile.isFollowing = !wasFollowing;
    try {
      if (wasFollowing) {
        await app.unfollowUser(accountId, profileState.profile.user.id);
      } else {
        await app.followUser(accountId, profileState.profile.user.id);
      }
    } catch (e) {
      profileState.profile.isFollowing = wasFollowing;
      followErr = String(e);
    } finally {
      followBusy = false;
    }
  }

  function addAsColumn() {
    if (profileState.status !== "ready") return;
    void app.addColumn(
      accountId,
      { type: "user", userId: profileState.profile.user.id },
      { kind: "keywords", value: [] },
      undefined,
      displayName(profileState.profile.user),
    );
    onclose();
  }

  $effect(() => {
    void load();
  });

  // Column.svelte のタイムライン無限スクロールと同じ閾値(残り300px)で追加取得する。
  function onNotesScroll(e: Event) {
    if (profileState.status !== "ready") return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      void loadMoreNotes(profileState.profile.user.id);
    }
  }
</script>

<Modal title="プロフィール" {onclose}>
  {#if profileState.status === "loading"}
    <p class="hint">読み込み中…</p>
  {:else if profileState.status === "error"}
    <p class="err">{profileState.message}</p>
    <button class="mini-btn" onclick={load}>再試行</button>
  {:else}
    {@const profile = profileState.profile}
    {#if profile.user.bannerUrl}
      <img class="banner" src={profile.user.bannerUrl} alt="" />
    {/if}
    <div class="head" class:with-banner={!!profile.user.bannerUrl}>
      {#if profile.user.avatarUrl}
        <img class="avatar" src={profile.user.avatarUrl} alt="" />
      {:else}
        <div class="avatar placeholder"></div>
      {/if}
      <div class="names">
        <span class="name"
          ><Mfm text={displayName(profile.user)} emojis={proxiedEmojiMap(profile.user.emojis, instanceHost)} simple
          /></span
        >
        <span class="acct">{acct(profile.user)}</span>
      </div>
      {#if !profile.isSelf}
        <button
          class="follow-btn"
          class:following={profile.isFollowing}
          onclick={toggleFollow}
          disabled={followBusy}
        >
          {profile.isFollowing ? "フォロー解除" : "フォロー"}
        </button>
      {/if}
    </div>
    {#if followErr}<p class="err">{followErr}</p>{/if}
    {#if profile.user.bio}
      <p class="bio"><Mfm text={profile.user.bio} emojis={proxiedEmojiMap(profile.user.emojis, instanceHost)} /></p>
    {/if}
    <div class="stats">
      <!-- aria-label で明示: "フォロー中" の文字列を含む accessible name にすると
           フォロー/フォロー解除トグルボタンを name=/フォロー/ で探すクエリと衝突するため -->
      <button class="stat-btn" aria-label="following-count" onclick={() => (followListKind = "following")}>
        <strong>{profile.user.followingCount}</strong> フォロー中
      </button>
      <button class="stat-btn" aria-label="followers-count" onclick={() => (followListKind = "followers")}>
        <strong>{profile.user.followersCount}</strong> フォロワー
      </button>
      <span class="stat-static"><strong>{profile.user.notesCount}</strong> ノート</span>
    </div>
    <button class="mini-btn add-column-btn" onclick={addAsColumn}>カラムとして追加</button>
    <div class="notes" onscroll={onNotesScroll}>
      {#each notes as note (note.id)}
        <NoteCard {note} {accountId} />
      {/each}
      {#if notesBusy}<p class="hint centered">読み込み中…</p>{/if}
      {#if notesErr}
        <p class="err">{notesErr}</p>
        <button class="mini-btn" onclick={() => loadMoreNotes(profile.user.id)} disabled={notesBusy}>再試行</button>
      {/if}
    </div>
    {#if followListKind}
      <FollowListModal
        kind={followListKind}
        userId={profile.user.id}
        {accountId}
        onclose={() => (followListKind = null)}
      />
    {/if}
  {/if}
</Modal>

<style>
  .hint {
    color: var(--text-dim);
    font-size: 0.85rem;
  }
  .hint.centered {
    text-align: center;
    margin: 0;
  }
  .banner {
    display: block;
    width: calc(100% + 32px);
    aspect-ratio: 3 / 1;
    object-fit: cover;
    border-radius: 6px;
    margin: 0 -16px;
  }
  .head {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    margin-top: 8px;
  }
  .head.with-banner {
    margin-top: -22px;
    padding-left: 4px;
  }
  .avatar {
    width: 56px;
    height: 56px;
    border-radius: 10px;
    object-fit: cover;
    flex: none;
    border: 2px solid var(--surface-1);
  }
  .avatar.placeholder {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
  .names {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    gap: 1px;
  }
  .name {
    font-weight: 600;
    font-size: 0.95rem;
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.78rem;
  }
  .follow-btn {
    flex: none;
    padding: 6px 16px;
    border: 1px solid var(--accent);
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 0.82rem;
    font-weight: 600;
    cursor: pointer;
  }
  .follow-btn:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .follow-btn.following {
    background: transparent;
    color: var(--text);
    border-color: var(--border);
  }
  .follow-btn.following:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }
  .follow-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .bio {
    margin: 10px 0 0;
    font-size: 0.88rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .stats {
    display: flex;
    gap: 4px;
    margin: 10px 0 0;
  }
  .stat-btn {
    padding: 3px 6px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-dim);
    font-size: 0.78rem;
    cursor: pointer;
  }
  .stat-btn:hover {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
    color: var(--text);
  }
  .stat-btn strong,
  .stat-static strong {
    color: var(--text);
    font-weight: 600;
  }
  .stat-static {
    padding: 3px 6px;
    font-size: 0.78rem;
    color: var(--text-dim);
  }
  .add-column-btn {
    margin-top: 10px;
  }
  .err {
    margin: 8px 0;
    color: var(--danger);
    font-size: 0.82rem;
  }
  .notes {
    max-height: 40vh;
    overflow-y: auto;
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .mini-btn {
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.8rem;
  }
  .mini-btn:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .mini-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
