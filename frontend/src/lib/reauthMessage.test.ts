import { describe, expect, it } from "vitest";
import { reauthMessage } from "./reauthMessage";

describe("reauthMessage", () => {
  it("トークンなし(設定のインポートで追加された)アカウントは、ログインを促す文言", () => {
    const m = reauthMessage(true);
    expect(m).toContain("設定のインポートで追加され");
    expect(m).toContain("まだログインしていません");
    expect(m).not.toContain("スコープ");
  });

  it("トークンはあるが再認証する(スコープ更新)ときは、従来の文言", () => {
    expect(reauthMessage(false)).toBe(
      "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。",
    );
  });
});
