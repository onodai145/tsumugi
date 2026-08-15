<script lang="ts">
  import type { Notification } from "../bindings/tauri.gen";
  import type { Component } from "svelte";
  import NoteCard from "./NoteCard.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import Mfm from "../render/Mfm.svelte";
  import { relativeTime } from "../lib/time";
  import { app } from "../lib/store.svelte";
  import { reactionEmoji, proxiedEmojiMap } from "../lib/emoji";
  import { openProfile } from "../lib/profileModal.svelte";
  import {
    UserPlus,
    MessageCircle,
    Repeat2,
    Quote,
    Star,
    Vote,
    Clock,
    UserCheck,
    Trophy,
    Bell,
  } from "@lucide/svelte";

  let { notification, accountId }: { notification: Notification; accountId?: string } = $props();
  const n = $derived(notification);

  const actor = $derived(n.user ? (n.user.name ?? n.user.username) : "");

  // カスタム絵文字（:name:）のみ解決。Unicode 絵文字はそのまま表示する。
  const instanceHost = $derived(accountId ? app.accounts.find((a) => a.id === accountId)?.host : undefined);
  // リアクション絵文字の解決用マップ: ローカル絵文字（閲覧インスタンス、そのままでよい）＋
  // 対象ノートの絵文字（生URLなのでプロキシ変換）。
  const emojiMap = $derived({
    ...(accountId ? app.localEmojiUrls(accountId) : {}),
    ...proxiedEmojiMap(n.note?.emojis, instanceHost),
  });
  const reaction = $derived(
    n.type === "reaction" && n.reaction?.startsWith(":")
      ? reactionEmoji(n.reaction, emojiMap, instanceHost)
      : null,
  );

  const labels: Record<string, string> = {
    follow: "にフォローされました",
    mention: "からメンション",
    reply: "から返信",
    renote: "がRenote",
    quote: "が引用",
    reaction: "がリアクション",
    pollEnded: "投票が終了",
    receiveFollowRequest: "からフォローリクエスト",
    followRequestAccepted: "がフォローを承認",
    achievementEarned: "実績を獲得",
    app: "アプリ通知",
  };
  const icons: Record<string, Component> = {
    follow: UserPlus,
    mention: MessageCircle,
    reply: MessageCircle,
    renote: Repeat2,
    quote: Quote,
    reaction: Star,
    pollEnded: Vote,
    receiveFollowRequest: Clock,
    followRequestAccepted: UserCheck,
    achievementEarned: Trophy,
  };
  const IconComp = $derived(icons[n.type] ?? Bell);
</script>

<article class="border-b border-border px-3 py-2 [content-visibility:auto] [contain-intrinsic-size:auto_80px]">
  <div class="flex items-center gap-2 text-sm">
    <span class="inline-flex flex-none text-muted-foreground"><IconComp size={16} /></span>
    {#if n.user?.avatarUrl}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <img
        class="h-6 w-6 flex-none rounded-md object-cover"
        data-testid="notification-avatar"
        src={n.user.avatarUrl}
        alt=""
        loading="lazy"
        onclick={() => n.user && openProfile({ userId: n.user.id }, accountId)}
        style="cursor: pointer"
      />
    {/if}
    <span class="min-w-0 flex-1">
      <!-- role="button"だがButtonプリミティブ非経由のため、キーボードフォーカス時の視認性を
           Buttonのfocus-visibleパターン（スタイルガイド§7、border-ringは無枠のため省略）で個別に補う -->
      {#if actor}<b
          class="rounded-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          data-testid="notification-actor"
          onclick={() => n.user && openProfile({ userId: n.user.id }, accountId)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === "Enter" && n.user && openProfile({ userId: n.user.id }, accountId)}
          style="cursor: pointer"
          ><Mfm text={actor} emojis={proxiedEmojiMap(n.user?.emojis, instanceHost)} simple /></b
        >{/if}
      {labels[n.type] ?? n.type}
      {#if n.type === "reaction" && n.reaction}
        <span class="ml-0.5">
          {#if reaction}
            <CustomEmoji name={reaction.name} url={reaction.url} />
          {:else}<UnicodeEmoji char={n.reaction} />{/if}
        </span>
      {/if}
    </span>
    <span class="text-sm text-muted-foreground">{relativeTime(n.createdAt)}</span>
  </div>
  {#if n.note}
    <div class="ml-[30px]" data-testid="notification-note-preview">
      <NoteCard
        note={n.note}
        quoted={true}
        showActions={true}
        hideReactions
        hideActionBanner
        accountId={accountId}
        emojiAccountId={accountId}
      />
    </div>
  {/if}
</article>
