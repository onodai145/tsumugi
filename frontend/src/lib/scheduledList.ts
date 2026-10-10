// 予約一覧(ScheduledModal)の行の型と、ローカル予約の表示用ヘルパ。Svelte に依存しない。
import type { LocalScheduleStatus, ScheduledNote } from "../bindings/tauri.gen";

/// 予約一覧の 1 行。サーバー予約(notes/drafts)とローカル予約(tsumugi が保持して投稿する)を同じ形で扱う。
export type ScheduledListItem =
  | { origin: "server"; note: ScheduledNote }
  | { origin: "local"; note: ScheduledNote; status: LocalScheduleStatus; error: string | null };

/// ローカル予約は、tsumugi が起動している間しか投稿できない。予約時と一覧で伝える。
export const LOCAL_SCHEDULE_NOTICE = "アプリを起動している間だけ投稿されます";

/// ローカル予約の状態の表示名。待機中(pending)は予約日時だけで足りるので表示しない。
export function localStatusLabel(status: LocalScheduleStatus): string | null {
  switch (status) {
    case "posting":
      return "投稿中";
    case "expired":
      return "期限切れ";
    case "failed":
      return "投稿に失敗";
    default:
      return null;
  }
}
