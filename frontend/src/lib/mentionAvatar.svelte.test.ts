import { afterEach, describe, expect, it, vi } from "vitest";

const resolveUserSilently = vi.fn();
const defaultAccountId = vi.fn();

vi.mock("./store.svelte", () => ({
  app: {
    defaultAccountId: (...args: unknown[]) => defaultAccountId(...args),
    resolveUserSilently: (...args: unknown[]) => resolveUserSilently(...args),
  },
}));

const { cachedAvatarUrl, fetchAvatarUrl } = await import("./mentionAvatar.svelte");

afterEach(() => {
  resolveUserSilently.mockReset();
  defaultAccountId.mockReset();
});

describe("mentionAvatar", () => {
  it("初期状態は未キャッシュ(undefined)", () => {
    expect(cachedAvatarUrl("fresh-user-1", null)).toBeUndefined();
  });

  it("解決に成功するとavatarUrlをキャッシュする", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserSilently.mockResolvedValue({
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
    });

    const result = await fetchAvatarUrl("alice-success", null);

    expect(result).toBe("https://example.com/alice.png");
    expect(cachedAvatarUrl("alice-success", null)).toBe("https://example.com/alice.png");
    expect(resolveUserSilently).toHaveBeenCalledWith("acc1", "alice-success");
  });

  it("解決に失敗した場合はnullをキャッシュし、以後は再フェッチしない", async () => {
    defaultAccountId.mockReturnValue("acc1");
    resolveUserSilently.mockRejectedValue(new Error("network error"));

    const result = await fetchAvatarUrl("bob-fail", null);

    expect(result).toBeNull();
    expect(cachedAvatarUrl("bob-fail", null)).toBeNull();

    // 2回目はキャッシュから返るのでresolveUserSilentlyは呼ばれない
    resolveUserSilently.mockClear();
    const second = await fetchAvatarUrl("bob-fail", null);
    expect(second).toBeNull();
    expect(resolveUserSilently).not.toHaveBeenCalled();
  });

  it("同一acctへの同時呼び出しは1回のリクエストに集約する（重複排除）", async () => {
    defaultAccountId.mockReturnValue("acc1");
    let resolveFn!: (v: unknown) => void;
    resolveUserSilently.mockReturnValue(
      new Promise((r) => {
        resolveFn = r;
      }),
    );

    const p1 = fetchAvatarUrl("carol-dedup", "remote.example");
    const p2 = fetchAvatarUrl("carol-dedup", "remote.example");
    expect(resolveUserSilently).toHaveBeenCalledTimes(1);
    expect(resolveUserSilently).toHaveBeenCalledWith("acc1", "carol-dedup@remote.example");

    resolveFn({
      id: "u2",
      username: "carol-dedup",
      host: "remote.example",
      name: null,
      avatarUrl: "https://remote.example/carol.png",
      isBot: false,
      isCat: false,
      followersCount: 0,
      followingCount: 0,
      notesCount: 0,
    });

    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1).toBe("https://remote.example/carol.png");
    expect(r2).toBe("https://remote.example/carol.png");
  });

  it("アカウント未設定（defaultAccountIdが空文字）の場合はnullを返しリクエストしない", async () => {
    defaultAccountId.mockReturnValue("");

    const result = await fetchAvatarUrl("dave-noaccount", null);

    expect(result).toBeNull();
    expect(resolveUserSilently).not.toHaveBeenCalled();
  });
});
