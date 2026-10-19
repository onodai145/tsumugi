// Tauri command のラッパ。生成された bindings の Result を unwrap し、
// 失敗時は型付き Error を Error オブジェクトに変換して throw する。
import { commands, type Error as ApiError, type HapticPattern } from "../bindings/tauri.gen";
import { resolveSoundChoice } from "./soundChoice";

export { commands };
export * from "../bindings/tauri.gen";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: ApiError };

export async function unwrap<T>(p: Promise<Result<T>>): Promise<T> {
  const r = await p;
  if (r.status === "ok") return r.data;
  throw new Error(formatError(r.error));
}

/// 403 (kind: "forbidden") を ForbiddenError として throw する unwrap。
/// 呼び出し元は accountId を渡し、store 側で「再認証」アクションをログに出せるようにする。
export class ForbiddenError extends Error {
  accountId: string;
  constructor(accountId: string, message: string) {
    super(message);
    this.name = "ForbiddenError";
    this.accountId = accountId;
  }
}

export async function unwrapAcc<T>(accountId: string, p: Promise<Result<T>>): Promise<T> {
  const r = await p;
  if (r.status === "ok") return r.data;
  if (r.error.kind === "forbidden") throw new ForbiddenError(accountId, formatError(r.error));
  throw new Error(formatError(r.error));
}

export function formatError(e: ApiError): string {
  return "message" in e ? `${e.kind}: ${e.message}` : e.kind;
}

export interface PlayNotifySoundOptions {
  /// 生成が終わった時点で true を返したら、鳴らさずに捨てる(「待たない」・編集された、など)。
  /// 捨てた生成の失敗は、beep も onError も出さない。
  isStale?: () => boolean;
  /// 鳴らした・捨てた・失敗した、のどれでも 1 回呼ぶ(MML の試聴の「生成中」表示を戻す)。
  onSettled?: () => void;
}

/// 通知音を鳴らす(Issue #12: 実際の再生は Rust 側の play_notify_sound コマンドで行う)。
/// 呼び出し側は結果を待つ必要がないため fire-and-forget。IPC 層自体の失敗(コマンド未登録等の
/// 開発時ミスなど)で unhandled promise rejection にならないよう、ここで一括して握りつぶす
/// (play_notify_sound コマンド自体は常に Ok を返す設計で、失敗は Rust 側で warn ログのみ)。
/// mml: は再生前に data: URL へ解決し、失敗したら beep で代替する(理由は onError に渡す)。
export function playNotifySound(choice: string, onError?: (e: unknown) => void, opts?: PlayNotifySoundOptions): void {
  void (async () => {
    try {
      let resolved = choice;
      try {
        resolved = await resolveSoundChoice(choice);
      } catch (e) {
        if (opts?.isStale?.()) return;
        onError?.(e);
        resolved = "beep"; // 解決できなくても通知は止めない
      }
      if (opts?.isStale?.()) return;
      await unwrap(commands.playNotifySound(resolved));
    } finally {
      opts?.onSettled?.();
    }
  })().catch(() => {});
}

/// ハプティクス(振動)を発火する(Issue #26)。isMobilePlatform && 設定ONの場合のみ呼ぶこと
/// (このガードは呼び出し側の責務。ここでは行わない)。IPC自体の失敗(未対応デバイス等)は
/// UXに影響しないため握りつぶす(playNotifySoundと同じfire-and-forgetパターン)。
export function vibrate(pattern: HapticPattern): void {
  void unwrap(commands.vibrate(pattern)).catch(() => {});
}

/// IME の変換候補ウィンドウの位置を補正する(Issue #502)。座標は viewport 座標(CSS px)。
/// Linux/Wayland 以外では Rust 側が何もしない。位置補正の失敗は入力に影響しないため握りつぶす
/// (vibrateと同じfire-and-forgetパターン)。
export function setImeCursorArea(rect: { x: number; y: number; width: number; height: number }): void {
  void unwrap(commands.setImeCursorArea(rect.x, rect.y, rect.width, rect.height)).catch(() => {});
}
