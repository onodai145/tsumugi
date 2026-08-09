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
  import { Button } from "$lib/components/ui/button";

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
    const u = await app.resolveUserSilently(accountId, acctStr);
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
  // notesErr が立っている間はスクロールのたびに再試行してしまう(失敗した直後は既に閾値内に
  // いるため)のを防ぐため、エラー時は無視する。再試行は下の再試行ボタン経由のみとする。
  function onNotesScroll(e: Event) {
    if (profileState.status !== "ready" || notesErr) return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      void loadMoreNotes(profileState.profile.user.id);
    }
  }
</script>

<Modal title="プロフィール" {onclose}>
  {#if profileState.status === "loading"}
    <p class="text-[0.85rem] text-muted-foreground">読み込み中…</p>
  {:else if profileState.status === "error"}
    <p class="my-2 text-[0.82rem] text-destructive">{profileState.message}</p>
    <Button variant="outline" size="sm" onclick={load}>再試行</Button>
  {:else}
    {@const profile = profileState.profile}
    {#if profile.user.bannerUrl}
      <img class="block aspect-[3/1] w-[calc(100%+32px)] -mx-4 rounded-md object-cover" src={profile.user.bannerUrl} alt="" />
    {/if}
    <div class={profile.user.bannerUrl ? "flex items-end gap-2.5 -mt-[22px] pl-1" : "mt-2 flex items-end gap-2.5"}>
      {#if profile.user.avatarUrl}
        <img class="h-14 w-14 flex-none rounded-[10px] border-2 border-background object-cover" src={profile.user.avatarUrl} alt="" />
      {:else}
        <div class="avatar-ph h-14 w-14 flex-none rounded-[10px] border-2 border-background"></div>
      {/if}
      <div class="flex min-w-0 flex-1 flex-col gap-px">
        <span class="text-[0.95rem] font-semibold"
          ><Mfm text={displayName(profile.user)} emojis={proxiedEmojiMap(profile.user.emojis, instanceHost)} simple
          /></span
        >
        <span class="text-[0.78rem] text-muted-foreground">{acct(profile.user)}</span>
      </div>
      {#if !profile.isSelf}
        <Button
          size="sm"
          variant={profile.isFollowing ? "outline" : "default"}
          class="flex-none rounded-full {profile.isFollowing ? 'hover:border-destructive hover:text-destructive' : ''}"
          disabled={followBusy}
          onclick={toggleFollow}
        >
          {profile.isFollowing ? "フォロー解除" : "フォロー"}
        </Button>
      {/if}
    </div>
    {#if followErr}<p class="my-2 text-[0.82rem] text-destructive">{followErr}</p>{/if}
    {#if profile.user.bio}
      <p class="mt-2.5 whitespace-pre-wrap break-words text-[0.88rem] leading-normal"
        ><Mfm text={profile.user.bio} emojis={proxiedEmojiMap(profile.user.emojis, instanceHost)} /></p
      >
    {/if}
    <div class="mt-2.5 flex gap-1">
      <!-- aria-label で明示: "フォロー中" の文字列を含む accessible name にすると
           フォロー/フォロー解除トグルボタンを name=/フォロー/ で探すクエリと衝突するため -->
      <Button variant="ghost" size="xs" aria-label="following-count" onclick={() => (followListKind = "following")}>
        <strong class="font-semibold text-foreground">{profile.user.followingCount}</strong> フォロー中
      </Button>
      <Button variant="ghost" size="xs" aria-label="followers-count" onclick={() => (followListKind = "followers")}>
        <strong class="font-semibold text-foreground">{profile.user.followersCount}</strong> フォロワー
      </Button>
      <span class="px-1.5 py-[3px] text-[0.78rem] text-muted-foreground"
        ><strong class="font-semibold text-foreground">{profile.user.notesCount}</strong> ノート</span
      >
    </div>
    <Button variant="outline" size="sm" class="mt-2.5" onclick={addAsColumn}>カラムとして追加</Button>
    <div
      class="mt-3 flex max-h-[40vh] flex-col gap-2 overflow-y-auto border-t border-border pt-2.5"
      data-testid="profile-notes-scroll"
      onscroll={onNotesScroll}
    >
      {#each notes as note (note.id)}
        <NoteCard {note} {accountId} />
      {/each}
      {#if notesBusy}<p class="m-0 text-center text-[0.85rem] text-muted-foreground">読み込み中…</p>{/if}
      {#if notesErr}
        <p class="my-2 text-[0.82rem] text-destructive">{notesErr}</p>
        <Button variant="outline" size="sm" onclick={() => loadMoreNotes(profile.user.id)} disabled={notesBusy}>再試行</Button>
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
  .avatar-ph {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
