<script lang="ts">
  import type { Notification } from "../bindings/tauri.gen";
  import type { Component } from "svelte";
  import NoteCard from "./NoteCard.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import UnicodeEmoji from "../render/UnicodeEmoji.svelte";
  import { relativeTime } from "../lib/time";
  import { app } from "../lib/store.svelte";
  import { reactionEmoji } from "../lib/emoji";
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

  // リアクション絵文字の解決用マップ: ローカル絵文字（閲覧インスタンス）＋対象ノートの絵文字。
  const emojiMap = $derived({
    ...(accountId ? app.localEmojiUrls(accountId) : {}),
    ...(n.note?.emojis ?? {}),
  });
  // カスタム絵文字（:name:）のみ解決。Unicode 絵文字はそのまま表示する。
  const reaction = $derived(
    n.type === "reaction" && n.reaction?.startsWith(":")
      ? reactionEmoji(n.reaction, emojiMap)
      : null,
  );

  const labels: Record<string, string> = {
    follow: "にフォローされました",
    mention: "からメンション",
    reply: "から返信",
    renote: "がRenote",
    quote: "が引用",
    reaction: "がリアクション",
    pollEnded: "の投票が終了",
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

<article class="notif">
  <div class="head">
    <span class="icon"><IconComp size={15} /></span>
    {#if n.user?.avatarUrl}
      <img class="avatar" src={n.user.avatarUrl} alt="" loading="lazy" />
    {/if}
    <span class="text">
      {#if actor}<b>{actor}</b>{/if}
      {labels[n.type] ?? n.type}
      {#if n.type === "reaction" && n.reaction}
        <span class="reaction">
          {#if reaction}
            <CustomEmoji name={reaction.name} url={reaction.url} />
          {:else}<UnicodeEmoji char={n.reaction} />{/if}
        </span>
      {/if}
    </span>
    <span class="time">{relativeTime(n.createdAt)}</span>
  </div>
  {#if n.note}
    <div class="note-preview">
      <NoteCard note={n.note} quoted={true} emojiAccountId={accountId} />
    </div>
  {/if}
</article>

<style>
  .notif {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    content-visibility: auto;
    contain-intrinsic-size: auto 80px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.86rem;
  }
  .icon {
    display: inline-flex;
    color: var(--text-dim);
    flex: none;
  }
  .avatar {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    object-fit: cover;
    flex: none;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .reaction {
    margin-left: 2px;
  }
  .time {
    color: var(--text-dim);
    font-size: 0.78rem;
  }
  .note-preview {
    margin-left: 30px;
  }
</style>
