<script lang="ts">
  import type { MutePeriod, Note } from "../bindings/tauri.gen";
  import type { ProfileTarget } from "../lib/profileModal.svelte";
  import { app } from "../lib/store.svelte";
  import { acct, displayName } from "../lib/userDisplay";
  import { proxiedEmojiMap } from "../lib/emoji";
  import Modal from "./Modal.svelte";
  import Mfm from "../render/Mfm.svelte";
  import NoteCard from "./NoteCard.svelte";
  import FollowListModal from "./FollowListModal.svelte";
  import Avatar from "./Avatar.svelte";
  import UserMenu from "./UserMenu.svelte";
  import { portal } from "../lib/portal";
  import { MoreHorizontal } from "@lucide/svelte";
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

  // 「…」メニュー(ミュート。Issue #545)。NoteCard のノートメニューと同じ portal + 固定配置。
  let userMenuOpen = $state(false);
  let userMenuAnchor = $state<HTMLElement | null>(null);
  let userMenuPos = $state<{ left: number; top: number } | null>(null);
  let muteBusy = $state(false);
  let muteErr = $state<string | null>(null);
  const USER_MENU_W = 200;
  $effect(() => {
    if (!userMenuOpen || !userMenuAnchor) return;
    const r = userMenuAnchor.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.right - USER_MENU_W), window.innerWidth - USER_MENU_W - 8);
    userMenuPos = { left, top: r.bottom + 6 };
  });

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
    muteErr = null;
    muteBusy = false;
    userMenuOpen = false;
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

  async function mute(period: MutePeriod) {
    if (profileState.status !== "ready" || muteBusy) return;
    const gen = requestGen;
    const userId = profileState.profile.user.id;
    userMenuOpen = false;
    muteBusy = true;
    muteErr = null;
    try {
      await app.muteUser(accountId, userId, period);
      if (gen === requestGen && profileState.status === "ready") profileState.profile.isMuted = true;
    } catch (e) {
      if (gen === requestGen) {
        // 表示が古かっただけ(すでにミュート中)なら、表示を直す。メニューで解除できるようになる。
        if (String(e).includes("ALREADY_MUTING") && profileState.status === "ready") {
          profileState.profile.isMuted = true;
          muteErr = "すでにミュート中です。表示を更新しました";
        } else {
          muteErr = String(e);
        }
      }
    } finally {
      if (gen === requestGen) muteBusy = false; // 切り替え後の新しいプロフィールの busy を、古い呼び出しが触らない
    }
  }

  async function unmute() {
    if (profileState.status !== "ready" || muteBusy) return;
    const gen = requestGen;
    const userId = profileState.profile.user.id;
    userMenuOpen = false;
    muteBusy = true;
    muteErr = null;
    try {
      await app.unmuteUser(accountId, userId);
      if (gen === requestGen && profileState.status === "ready") profileState.profile.isMuted = false;
    } catch (e) {
      if (gen === requestGen) {
        // 期限切れなどで、すでにミュート中でなかったなら、表示を直す。
        if (String(e).includes("NOT_MUTING") && profileState.status === "ready") {
          profileState.profile.isMuted = false;
          muteErr = "すでにミュート中ではありません。表示を更新しました";
        } else {
          muteErr = String(e);
        }
      }
    } finally {
      if (gen === requestGen) muteBusy = false;
    }
  }

  // メニューを開いているときの Escape は、メニューだけ閉じる(モーダルまで閉じない)。メニューは portal で
  // モーダルの外にあるので、フォーカスがどちらにあっても効くよう window の捕捉で受ける。
  function onWindowKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || !userMenuOpen) return;
    userMenuOpen = false;
    e.stopPropagation();
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
  const mobileUi = $derived(app.useMobileUi());

  function onNotesScroll(e: Event) {
    if (profileState.status !== "ready" || notesErr) return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      void loadMoreNotes(profileState.profile.user.id);
    }
  }
</script>

<svelte:window onkeydowncapture={onWindowKeydown} />

<Modal title="プロフィール" {onclose} maxHeight={mobileUi ? "100%" : undefined}>
  <!-- モバイルは全画面ページ。プロフィール全体を1つのスクロール領域にし(ノート一覧の自前スクロールは
       やめる)、無限スクロールもこの領域のscrollで行う(Issue #450)。PCでは素通し(display: contents)で
       従来どおりノート一覧が自前スクロールする。同じ要素のままクラスだけ切り替えるのは、{#if}で
       分岐すると切替時に中身が作り直されるため。 -->
  <div
    class={mobileUi ? "-mx-4 min-h-0 flex-1 overflow-y-auto px-4" : "contents"}
    data-testid="profile-scroll"
    onscroll={mobileUi ? onNotesScroll : undefined}
  >
    {#if profileState.status === "loading"}
      <p class="text-sm text-muted-foreground">読み込み中…</p>
    {:else if profileState.status === "error"}
      <p class="my-2 text-sm text-destructive">{profileState.message}</p>
      <Button variant="outline" size="sm" onclick={load}>再試行</Button>
    {:else}
      {@const profile = profileState.profile}
      {#if profile.user.bannerUrl}
        <img class="block aspect-[3/1] w-[calc(100%+32px)] -mx-4 rounded-md object-cover" src={profile.user.bannerUrl} alt="" />
      {/if}
      <div class={profile.user.bannerUrl ? "flex items-end gap-2.5 -mt-[22px] pl-1" : "mt-2 flex items-end gap-2.5"}>
        <Avatar
          isCat={profile.user.isCat}
          avatarBlurhash={profile.user.avatarBlurhash}
          class="h-14 w-14 flex-none"
        >
          {#if profile.user.avatarUrl}
            <img class="h-full w-full rounded-[var(--avatar-radius,20%)] border-2 border-background object-cover" src={profile.user.avatarUrl} alt="" />
          {:else}
            <div class="avatar-ph h-full w-full rounded-[var(--avatar-radius,20%)] border-2 border-background"></div>
          {/if}
        </Avatar>
        <div class="flex min-w-0 flex-1 flex-col gap-px">
          <span class="text-sm font-semibold"
            ><Mfm text={displayName(profile.user)} emojis={proxiedEmojiMap(profile.user.emojis, instanceHost)} simple
            /></span
          >
          <span class="text-sm text-muted-foreground">{acct(profile.user)}</span>
          {#if profile.isMuted}
            <span class="inline-flex w-fit items-center rounded-sm border border-border px-1 text-xs text-muted-foreground">ミュート中</span>
          {/if}
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
          {#if profile.isMuted !== null}
            <span class="flex-none" bind:this={userMenuAnchor}>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="ユーザーメニュー"
                disabled={muteBusy}
                onclick={() => (userMenuOpen = !userMenuOpen)}
              >
                <MoreHorizontal size={16} />
              </Button>
            </span>
            {#if userMenuOpen && userMenuPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (userMenuOpen = false)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="fixed"
                  style={`left:${userMenuPos.left}px;top:${userMenuPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <UserMenu muted={profile.isMuted} onmute={mute} onunmute={unmute} />
                </div>
              </div>
            {/if}
          {/if}
        {/if}
      </div>
      {#if followErr}<p class="my-2 text-sm text-destructive">{followErr}</p>{/if}
      {#if muteErr}<p class="my-2 text-sm text-destructive">{muteErr}</p>{/if}
      {#if profile.user.bio}
        <p class="mt-2.5 mb-0 whitespace-pre-wrap break-words text-sm leading-normal"
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
        <span class="px-1.5 py-[3px] text-sm text-muted-foreground"
          ><strong class="font-semibold text-foreground">{profile.user.notesCount}</strong> ノート</span
        >
      </div>
      <Button variant="outline" size="sm" class="mt-2.5" onclick={addAsColumn}>カラムとして追加</Button>
      <div
        class={mobileUi
        ? "mt-3 flex flex-col gap-2 border-t border-border pt-2.5"
        : "mt-3 flex max-h-[40vh] flex-col gap-2 overflow-y-auto border-t border-border pt-2.5"}
        data-testid="profile-notes-scroll"
        onscroll={onNotesScroll}
      >
        {#each notes as note (note.id)}
          <NoteCard {note} {accountId} />
        {/each}
        {#if notesBusy}<p class="m-0 text-center text-sm text-muted-foreground">読み込み中…</p>{/if}
        {#if notesErr}
          <p class="my-2 text-sm text-destructive">{notesErr}</p>
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
  </div>
</Modal>

<style>
  .avatar-ph {
    background: color-mix(in srgb, var(--surface-3) var(--column-opacity, 100%), transparent);
  }
</style>
