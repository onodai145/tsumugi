import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../bindings/tauri.gen", () => ({ commands: { speakNote: vi.fn() } }));
vi.mock("./soundChoice", () => ({ resolveSoundChoice: vi.fn() }));

import { commands } from "../bindings/tauri.gen";
import { speakNote } from "./ipc";

const speak = vi.mocked(commands.speakNote);

beforeEach(() => {
  speak.mockReset();
  speak.mockResolvedValue({ status: "ok", data: null });
});

describe("speakNote", () => {
  it("columnId / noteId / text をそのまま speakNote コマンドへ渡す", async () => {
    speakNote("c1", "n1", "こんにちは");
    await vi.waitFor(() => expect(speak).toHaveBeenCalledWith("c1", "n1", "こんにちは"));
  });

  it("コマンドが error を返しても例外を投げず、onError に渡す", async () => {
    speak.mockResolvedValue({ status: "error", error: { kind: "invalid", message: "SPEECH_NO_VOICE" } });
    const onError = vi.fn();
    expect(() => speakNote("c1", "n1", "x", onError)).not.toThrow();
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());
  });

  it("invoke 自体が reject しても unhandled rejection にならない", async () => {
    speak.mockRejectedValue(new Error("boom"));
    const onError = vi.fn();
    speakNote("c1", "n1", "x", onError);
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());
  });

  it("onError を渡さなくても、失敗で落ちない", async () => {
    speak.mockRejectedValue(new Error("boom"));
    speakNote("c1", "n1", "x");
    await new Promise((r) => setTimeout(r, 0));
    expect(speak).toHaveBeenCalledTimes(1);
  });
});
