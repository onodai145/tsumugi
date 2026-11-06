import { describe, expect, it, vi } from "vitest";

vi.mock("../bindings/tauri.gen", () => ({ commands: {} }));
vi.mock("./soundChoice", () => ({ resolveSoundChoice: vi.fn() }));

import { AppError, ForbiddenError, formatError, unwrap, unwrapAcc } from "./ipc";

const err = (error: unknown) => Promise.resolve({ status: "error" as const, error: error as never });

async function caught(p: Promise<unknown>): Promise<unknown> {
  try {
    await p;
  } catch (e) {
    return e;
  }
  throw new Error("rejectされなかった");
}

describe("formatError", () => {
  it("kind の接頭辞を付けず、message だけを返す", () => {
    expect(formatError({ kind: "invalid", message: "tsumugiの設定ファイルではありません" })).toBe(
      "tsumugiの設定ファイルではありません",
    );
    expect(formatError({ kind: "network", message: "connect: refused" })).toBe("connect: refused");
  });

  it("message を持たない rateLimited は日本語にする", () => {
    expect(formatError({ kind: "rateLimited" })).toBe("リクエストが多すぎます。しばらく待ってからやり直してください");
  });
});

describe("unwrap / unwrapAcc が投げるエラー", () => {
  it("unwrap: String(e) が message だけになる(Error: も kind も付かない)", async () => {
    const e = await caught(unwrap(err({ kind: "invalid", message: "tsumugiの設定ファイルではありません" })));
    expect(e).toBeInstanceOf(AppError);
    expect(String(e)).toBe("tsumugiの設定ファイルではありません");
  });

  it("unwrapAcc: 403 は ForbiddenError(AppError の一種)で、accountId を持つ。String(e) は message だけ", async () => {
    const e = await caught(unwrapAcc("acc1", err({ kind: "forbidden", message: "権限が足りません" })));
    expect(e).toBeInstanceOf(ForbiddenError);
    expect(e).toBeInstanceOf(AppError);
    expect((e as ForbiddenError).accountId).toBe("acc1");
    expect(String(e)).toBe("権限が足りません");
  });

  it("unwrapAcc: 403 以外は AppError", async () => {
    const e = await caught(unwrapAcc("acc1", err({ kind: "network", message: "offline" })));
    expect(e).toBeInstanceOf(AppError);
    expect(e).not.toBeInstanceOf(ForbiddenError);
    expect(String(e)).toBe("offline");
  });

  it("IPC を経由しない通常のJS例外は、従来どおり Error: 付きで出る(バグと見分けるため)", () => {
    expect(String(new Error("boom"))).toBe("Error: boom");
  });
});
