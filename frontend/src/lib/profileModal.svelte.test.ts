import { afterEach, describe, expect, it } from "vitest";
import {
  closeProfile,
  currentProfileAccountId,
  currentProfileTarget,
  openProfile,
} from "./profileModal.svelte";

afterEach(() => closeProfile());

describe("profileModal store", () => {
  it("初期状態はnull", () => {
    expect(currentProfileTarget()).toBeNull();
    expect(currentProfileAccountId()).toBeNull();
  });

  it("openProfileでターゲットとaccountIdが設定される", () => {
    openProfile({ userId: "u1" }, "acc1");
    expect(currentProfileTarget()).toEqual({ userId: "u1" });
    expect(currentProfileAccountId()).toBe("acc1");
  });

  it("accountId省略時はnull", () => {
    openProfile({ username: "alice", host: "example.com" });
    expect(currentProfileTarget()).toEqual({ username: "alice", host: "example.com" });
    expect(currentProfileAccountId()).toBeNull();
  });

  it("closeProfileで両方nullに戻る", () => {
    openProfile({ userId: "u1" }, "acc1");
    closeProfile();
    expect(currentProfileTarget()).toBeNull();
    expect(currentProfileAccountId()).toBeNull();
  });
});
