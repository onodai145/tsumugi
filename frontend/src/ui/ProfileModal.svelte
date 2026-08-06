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
</script>

<Modal title="プロフィール" {onclose}>
  {#if profileState.status === "loading"}
    <p>読み込み中…</p>
  {:else if profileState.status === "error"}
    <p class="err">{profileState.message}</p>
    <button onclick={load}>再試行</button>
  {:else}
    {@const profile = profileState.profile}
    {#if profile.user.bannerUrl}
      <img class="banner" src={profile.user.bannerUrl} alt="" />
    {/if}
    <div class="head">
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
        <button onclick={toggleFollow} disabled={followBusy}>
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
      <button aria-label="following-count" onclick={() => (followListKind = "following")}
        >フォロー中 {profile.user.followingCount}</button
      >
      <button aria-label="followers-count" onclick={() => (followListKind = "followers")}
        >フォロワー {profile.user.followersCount}</button
      >
      <span>ノート {profile.user.notesCount}</span>
    </div>
    <button onclick={addAsColumn}>カラムとして追加</button>
    <div class="notes">
      {#each notes as note (note.id)}
        <NoteCard {note} {accountId} />
      {/each}
      {#if notesErr}
        <p class="err">{notesErr}</p>
        <button onclick={() => loadMoreNotes(profile.user.id)} disabled={notesBusy}>再試行</button>
      {:else if !notesDone}
        <button onclick={() => loadMoreNotes(profile.user.id)} disabled={notesBusy}>もっと見る</button>
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
  .banner {
    width: 100%;
    aspect-ratio: 3 / 1;
    object-fit: cover;
    border-radius: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .avatar {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    object-fit: cover;
  }
  .avatar.placeholder {
    background: var(--surface-2);
  }
  .names {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .acct {
    color: var(--text-dim);
    font-size: 0.85em;
  }
  .stats {
    display: flex;
    gap: 12px;
    margin: 8px 0;
  }
  .err {
    color: var(--danger, #d33);
  }
  .notes {
    max-height: 40vh;
    overflow-y: auto;
    margin-top: 8px;
  }
</style>
