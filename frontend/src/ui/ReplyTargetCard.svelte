<script lang="ts">
  import type { Note } from "../bindings/tauri.gen";
  import Mfm from "../render/Mfm.svelte";
  import Avatar from "./Avatar.svelte";
  import { relativeTime } from "../lib/time";
  import { acct, displayName } from "../lib/userDisplay";
  import { replyCardBody } from "../lib/replyPreview";
  import { proxiedEmojiMap } from "../lib/emoji";
  import { app } from "../lib/store.svelte";
  import { openProfile } from "../lib/profileModal.svelte";

  // 返信ノートの上に出す返信先カード(Issue #287、カラム/通知カード用)。
  // 展開トグルは持たない。アバター/表示名のクリックで返信先ユーザーのプロフィールを開く。
  // emojiMap は本文用(閲覧インスタンスのローカル絵文字+返信先の絵文字を解決済みのもの)、
  // instanceHost は表示名用の絵文字プロキシ解決に使う。
  let {
    note,
    accountId,
    emojiMap = {},
    instanceHost,
  }: {
    note: Note;
    accountId?: string;
    emojiMap?: Record<string, string>;
    instanceHost?: string;
  } = $props();

  const body = $derived(replyCardBody(note));

  // app.now を依存として読み、時間経過に合わせて相対時刻を再計算させる(NoteCard の displayTime と同じ)。
  const displayTime = $derived.by(() => {
    app.now;
    return relativeTime(note.createdAt);
  });
</script>

<!-- NoteCard 本体のアバター/名前と同じ操作パターン(クリックでプロフィール)。img はキーボード操作の対象にしない -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="mb-0.5 flex min-w-0 gap-1.5 text-muted-foreground" data-testid="reply-card">
  <Avatar isCat={note.user.isCat} avatarBlurhash={note.user.avatarBlurhash} class="mt-0.5 h-6 w-6 flex-none">
    {#if note.user.avatarUrl}
      <img
        class="h-full w-full rounded-[var(--avatar-radius,20%)] object-cover"
        data-testid="reply-card-avatar"
        src={note.user.avatarUrl}
        alt=""
        loading="lazy"
        onclick={() => openProfile({ userId: note.user.id }, accountId)}
        style="cursor: pointer"
      />
    {:else}
      <!-- role="button" だが Button プリミティブ非経由のため、focus-visible はスタイルガイド§7に従い個別に補う -->
      <div
        class="avatar h-full w-full rounded-[var(--avatar-radius,20%)] outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
        data-testid="reply-card-avatar"
        onclick={() => openProfile({ userId: note.user.id }, accountId)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === "Enter" && openProfile({ userId: note.user.id }, accountId)}
      ></div>
    {/if}
  </Avatar>
  <div class="min-w-0 flex-1">
    <div class="flex min-w-0 items-baseline gap-[5px] text-xs">
      <span
        class="min-w-0 truncate rounded-sm font-semibold outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
        data-testid="reply-card-name"
        onclick={() => openProfile({ userId: note.user.id }, accountId)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === "Enter" && openProfile({ userId: note.user.id }, accountId)}
        style="cursor: pointer"
      ><Mfm
        text={displayName(note.user)}
        emojis={proxiedEmojiMap(note.user.emojis, instanceHost)}
        simple
      /></span>
      <span class="min-w-0 truncate" data-testid="reply-card-acct">{acct(note.user)}</span>
      <span
        class="ml-auto flex-none"
        data-testid="reply-card-time"
        title={new Date(note.createdAt * 1000).toLocaleString()}
      >{displayTime}</span>
    </div>
    {#if body}
      <div
        class="line-clamp-3 whitespace-pre-wrap break-words text-xs"
        data-testid="reply-card-body"
      >{#if body.kind === "label"}{body.label}{:else}<Mfm text={body.text} emojis={emojiMap} simple />{/if}</div>
    {/if}
  </div>
</div>
