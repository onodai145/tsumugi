<script lang="ts">
  import type { Notification } from "../bindings/tauri.gen";
  import NoteCard from "./NoteCard.svelte";
  import CustomEmoji from "../render/CustomEmoji.svelte";
  import { relativeTime } from "../lib/time";

  let { notification }: { notification: Notification } = $props();
  const n = $derived(notification);

  const actor = $derived(n.user ? (n.user.name ?? n.user.username) : "");

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
  const icons: Record<string, string> = {
    follow: "➕",
    mention: "💬",
    reply: "💬",
    renote: "🔁",
    quote: "❝",
    reaction: "⭐",
    pollEnded: "📊",
    receiveFollowRequest: "❓",
    followRequestAccepted: "✅",
    achievementEarned: "🏆",
  };
</script>

<article class="notif">
  <div class="head">
    <span class="icon">{icons[n.type] ?? "🔔"}</span>
    {#if n.user?.avatarUrl}
      <img class="avatar" src={n.user.avatarUrl} alt="" loading="lazy" />
    {/if}
    <span class="text">
      {#if actor}<b>{actor}</b>{/if}
      {labels[n.type] ?? n.type}
      {#if n.type === "reaction" && n.reaction}
        <span class="reaction">
          {#if n.reaction.startsWith(":")}
            <CustomEmoji name={n.reaction.replace(/^:|:$/g, "")} />
          {:else}{n.reaction}{/if}
        </span>
      {/if}
    </span>
    <span class="time">{relativeTime(n.createdAt)}</span>
  </div>
  {#if n.note}
    <div class="note-preview">
      <NoteCard note={n.note} quoted={true} />
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
    font-size: 0.95rem;
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
