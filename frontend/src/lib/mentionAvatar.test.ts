import { afterEach, describe, expect, it, vi } from "vitest";

const resolveUserAcct = vi.fn();
const defaultAccountId = vi.fn();

vi.mock("./ipc", () => ({
  commands: {
    resolveUserAcct: (...args: unknown[]) => resolveUserAcct(...args),
  },
}));

vi.mock("./store.svelte", () => ({
  app: {
    defaultAccountId: (...args: unknown[]) => defaultAccountId(...args),
  },
}));

const { cachedAvatarUrl, fetchAvatarUrl } = await import("./mentionAvatar");

function user(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    id: "u1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: "https://example.com/alice.png",
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    ...overrides,
  };
}

afterEach(() => {
  resolveUserAcct.mockReset();
  defaultAccountId.mockReset();
});

describe("mentionAvatar", () => {
  it("初期状態は未キャッシュ(undefined)", () => {
    defaultAccountId.mockReturnValue("acc1");
    expect(cachedAvatarUrl("fresh-user-1", null)).toBeUndefined();
  });

  it("解決に成功するとavatarUrlをキャッシュする", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserAcct.mockResolvedValue({ status: "ok", data: user({ username: "alice-success" }) });

    const result = await fetchAvatarUrl("alice-success", null);

    expect(result).toBe("https://example.com/alice.png");
    expect(cachedAvatarUrl("alice-success", null)).toBe("https://example.com/alice.png");
    expect(resolveUserAcct).toHaveBeenCalledWith("acc1", "alice-success");
  });

  it("NotFound(解決失敗)の場合はnullをキャッシュし、以後は再フェッチしない", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserAcct.mockResolvedValue({ status: "error", error: { kind: "notFound", message: "no such user" } });

    const result = await fetchAvatarUrl("bob-notfound", null);

    expect(result).toBeNull();
    expect(cachedAvatarUrl("bob-notfound", null)).toBeNull();

    // 2回目はキャッシュから返るのでresolveUserAcctは呼ばれない
    resolveUserAcct.mockClear();
    const second = await fetchAvatarUrl("bob-notfound", null);
    expect(second).toBeNull();
    expect(resolveUserAcct).not.toHaveBeenCalled();
  });

  it("レート制限等の一時的なエラーはキャッシュせず、次回呼び出しで再試行する", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserAcct.mockResolvedValueOnce({ status: "error", error: { kind: "rateLimited" } });

    const first = await fetchAvatarUrl("carol-ratelimited", null);
    expect(first).toBeNull();
    expect(cachedAvatarUrl("carol-ratelimited", null)).toBeUndefined();

    resolveUserAcct.mockResolvedValueOnce({ status: "ok", data: user({ username: "carol-ratelimited" }) });
    const second = await fetchAvatarUrl("carol-ratelimited", null);

    expect(second).toBe("https://example.com/alice.png");
    expect(resolveUserAcct).toHaveBeenCalledTimes(2);
  });

  it("IPC呼び出し自体が例外を投げた場合も一時的エラーとして扱い、キャッシュしない", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserAcct.mockRejectedValueOnce(new Error("command not registered"));

    const result = await fetchAvatarUrl("dave-ipcerror", null);

    expect(result).toBeNull();
    expect(cachedAvatarUrl("dave-ipcerror", null)).toBeUndefined();
  });

  it("同一acctへの同時呼び出しは1回のリクエストに集約する（重複排除）", async () => {
    defaultAccountId.mockReturnValue("acc1");
    let resolveFn!: (v: unknown) => void;
    resolveUserAcct.mockReturnValue(
      new Promise((r) => {
        resolveFn = r;
      }),
    );

    const p1 = fetchAvatarUrl("carol-dedup", "remote.example");
    const p2 = fetchAvatarUrl("carol-dedup", "remote.example");
    expect(resolveUserAcct).toHaveBeenCalledTimes(1);
    expect(resolveUserAcct).toHaveBeenCalledWith("acc1", "carol-dedup@remote.example");

    resolveFn({
      status: "ok",
      data: user({ username: "carol-dedup", host: "remote.example", avatarUrl: "https://remote.example/carol.png" }),
    });

    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1).toBe("https://remote.example/carol.png");
    expect(r2).toBe("https://remote.example/carol.png");
  });

  it("アカウント未設定（defaultAccountIdが空文字）の場合はnullを返しリクエストしない", async () => {
    defaultAccountId.mockReturnValue("");

    const result = await fetchAvatarUrl("erin-noaccount", null);

    expect(result).toBeNull();
    expect(resolveUserAcct).not.toHaveBeenCalled();
  });

  it("同一acctでもアカウントが異なればキャッシュを共有しない", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserAcct.mockResolvedValueOnce({
      status: "ok",
      data: user({ username: "frank", avatarUrl: "https://example.com/frank-acc1.png" }),
    });
    await fetchAvatarUrl("frank", null);
    expect(cachedAvatarUrl("frank", null)).toBe("https://example.com/frank-acc1.png");

    defaultAccountId.mockReturnValue("acc2");
    expect(cachedAvatarUrl("frank", null)).toBeUndefined();

    resolveUserAcct.mockResolvedValueOnce({
      status: "ok",
      data: user({ username: "frank", avatarUrl: "https://example.com/frank-acc2.png" }),
    });
    const result = await fetchAvatarUrl("frank", null);
    expect(result).toBe("https://example.com/frank-acc2.png");
    expect(cachedAvatarUrl("frank", null)).toBe("https://example.com/frank-acc2.png");

    defaultAccountId.mockReturnValue("acc1");
    expect(cachedAvatarUrl("frank", null)).toBe("https://example.com/frank-acc1.png");
  });
});
