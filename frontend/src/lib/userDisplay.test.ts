import { describe, expect, it } from "vitest";
import { acct, displayName } from "./userDisplay";

describe("acct", () => {
  it("ローカルユーザーは @username", () => {
    expect(acct({ username: "alice", host: null })).toBe("@alice");
  });
  it("リモートユーザーは @username@host", () => {
    expect(acct({ username: "alice", host: "example.com" })).toBe("@alice@example.com");
  });
});

describe("displayName", () => {
  it("nameがあればnameを返す", () => {
    expect(displayName({ name: "Alice", username: "alice" })).toBe("Alice");
  });
  it("nameがnullならusernameを返す", () => {
    expect(displayName({ name: null, username: "alice" })).toBe("alice");
  });
});
