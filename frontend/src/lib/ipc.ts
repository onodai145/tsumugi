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

export function formatError(e: ApiError): string {
  return "message" in e ? `${e.kind}: ${e.message}` : e.kind;
}
