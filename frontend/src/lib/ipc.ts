// Tauri command のラッパ。生成された bindings の Result を unwrap し、
// 失敗時は型付き Error を Error オブジェクトに変換して throw する。
import { commands, type Error as ApiError } from "../bindings/tauri.gen";

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

/// 通知音を鳴らす(Issue #12: 実際の再生は Rust 側の play_notify_sound コマンドで行う)。
/// 呼び出し側は結果を待つ必要がないため fire-and-forget。IPC 層自体の失敗(コマンド未登録等の
/// 開発時ミスなど)で unhandled promise rejection にならないよう、ここで一括して握りつぶす
/// (play_notify_sound コマンド自体は常に Ok を返す設計で、失敗は Rust 側で warn ログのみ)。
export function playNotifySound(choice: string): void {
  void unwrap(commands.playNotifySound(choice)).catch(() => {});
}
