// 予約投稿(Issue #60)の日時変換・検証・エラーメッセージ。Svelte に依存しない純粋関数。

/// `<input type="datetime-local">` の値(ローカルタイムゾーン)を epoch 秒にする。空・不正は null。
export function localInputToEpochSec(value: string): number | null {
  if (!value) return null;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

/// epoch 秒を `<input type="datetime-local">` の値にする。toISOString()(UTC)ではなく
/// ローカル成分から組み立てる(でないとタイムゾーン分ずれる)。
export function epochSecToLocalInput(sec: number): string {
  const dt = new Date(sec * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${dt.getFullYear()}-${pad(dt.getMonth() + 1)}-${pad(dt.getDate())}T${pad(dt.getHours())}:${pad(dt.getMinutes())}`;
}

export type ScheduleCheck = { ok: true } | { ok: false; message: string };

/// 送信前の検証。`pollExpiresAtMs` は投票の締切(ms)、投票なし・無期限は null。
/// 締切が予約日時以前だと、投稿された瞬間に期限切れの投票になるため拒否する。
export function validateSchedule(
  scheduledAtSec: number,
  nowMs: number,
  pollExpiresAtMs: number | null,
): ScheduleCheck {
  const scheduledMs = scheduledAtSec * 1000;
  if (scheduledMs <= nowMs) {
    return { ok: false, message: "予約日時は現在より後にしてください" };
  }
  if (pollExpiresAtMs != null && pollExpiresAtMs <= scheduledMs) {
    return { ok: false, message: "投票の締切が予約日時以前です。締切を予約日時より後にしてください" };
  }
  return { ok: true };
}

/// 本家 Misskey の予約まわりのエラーコードを日本語にする。該当しなければ元の文字列を返す。
export function scheduleErrorMessage(message: string): string {
  if (message.includes("TOO_MANY_SCHEDULED_NOTES")) {
    return "予約できる投稿数の上限に達しています。予約一覧から取り消してください";
  }
  if (message.includes("SCHEDULED_AT_MUST_BE_IN_FUTURE")) {
    return "予約日時は現在より後にしてください";
  }
  if (message.includes("SCHEDULED_AT_REQUIRED")) {
    return "予約日時を指定してください";
  }
  return message;
}
