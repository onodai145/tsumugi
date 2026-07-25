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
