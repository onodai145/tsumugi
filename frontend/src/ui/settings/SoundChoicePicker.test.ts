import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, within } from "@testing-library/svelte";
import { EditorView } from "@codemirror/view";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { commands } = await import("../../lib/ipc");
const { default: SoundChoicePicker } = await import("./SoundChoicePicker.svelte");
const { TIMBRE_LIST } = await import("../../lib/mmlTimbres");
const { MML_SAMPLES } = await import("../../lib/mmlSamples");

const okScore = { notes: [], totalSec: 2.5, trackCount: 1 };

/** MmlEditor は動的 import なので、現れるまで待ってから EditorView を取る */
async function viewOf(findByTestId: (id: string) => Promise<HTMLElement>): Promise<EditorView> {
  const view = EditorView.findFromDOM(await findByTestId("mml-input"));
  if (!view) throw new Error("EditorView が見つからない");
  return view;
}

function setDoc(view: EditorView, text: string) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({ status: "parsed", score: okScore });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SoundChoicePicker の MML モード", () => {
  it("保存済みの mml: を開くと MML モードで、入力欄に本文が入る", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:t120 o5 c4" });
    expect((await viewOf(findByTestId)).state.doc.toString()).toBe("t120 o5 c4");
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("2.5"));
  });

  it("構文が通れば再生時間を出し、invalid=false を通知する", async () => {
    const onvalidity = vi.fn();
    render(SoundChoicePicker, { value: "mml:c4", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(false));
  });

  it("構文エラーは位置(1 始まり)と理由を出し、invalid=true を通知する", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue({
      status: "invalid",
      error: { position: 3, message: "不明な文字 `x` です" },
    });
    const onvalidity = vi.fn();
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4 x", onvalidity });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("4"));
    expect(getByTestId("mml-status").textContent).toContain("不明な文字");
    expect(getByTestId("mml-status").textContent).toContain("4 文字目: 不明な文字");
    expect(onvalidity).toHaveBeenLastCalledWith(true);
  });

  it("parse_mml の IPC が失敗したら、位置なしのメッセージを出し invalid=true のままにする(未処理の reject はテスト失敗になる)", async () => {
    vi.spyOn(commands, "parseMml").mockRejectedValue(new Error("ipc down"));
    const onvalidity = vi.fn();
    const { getByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:c4", onvalidity });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("ipc down"));
    const text = getByTestId("mml-status").textContent ?? "";
    expect(text).toContain("構文を確認できませんでした");
    expect(text).not.toContain("文字目");
    expect(text).not.toContain("確認中");
    expect(queryByTestId("mml-error-jump")).toBeNull();
    expect(onvalidity).toHaveBeenLastCalledWith(true);
  });

  it("本文が空なら parse_mml を呼ばずに invalid=true にする", async () => {
    const onvalidity = vi.fn();
    render(SoundChoicePicker, { value: "mml:", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(true));
    expect(commands.parseMml).not.toHaveBeenCalled();
  });

  it("入力すると 300ms 待って、最後のテキストだけをパースする", async () => {
    const { findByTestId } = render(SoundChoicePicker, { value: "mml:c" });
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenCalledTimes(1));
    const view = await viewOf(findByTestId);
    setDoc(view, "cd");
    setDoc(view, "cde");
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenLastCalledWith("cde"));
    expect(commands.parseMml).toHaveBeenCalledTimes(2); // "c" と "cde"(途中の "cd" は呼ばれない)
  });

  it("プリセットのときは MML の入力欄を出さない", () => {
    const { queryByTestId } = render(SoundChoicePicker, { value: "chime" });
    expect(queryByTestId("mml-input")).toBeNull();
  });

  it("MML モードを抜けると invalid=false に戻す(親の保存ボタンを止めたままにしない)", async () => {
    const onvalidity = vi.fn();
    const { unmount } = render(SoundChoicePicker, { value: "mml:", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(true));
    unmount();
    expect(onvalidity).toHaveBeenLastCalledWith(false);
  });

  it("記法ヘルプに音色の一覧(番号と名前)が出る", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-timbres").textContent ?? "";
    expect(TIMBRE_LIST).toHaveLength(14);
    for (const p of TIMBRE_LIST) expect(text).toContain(`@${p.number} ${p.name}`);
  });

  it("記法ヘルプに、タイ ^・相対音量・q・連符・ループの脱出とネスト・コメントの書き方が出る", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    for (const s of ["c4^8", "v+2", "q80", "{cde}4", "[c:d]3", "//", "/* … */"]) expect(text).toContain(s);
    expect(text).not.toContain("2〜99回、入れ子は不可"); // 繰り返しは入れ子にできるようになった
    expect(text).toContain("全体で 5000 文字");
  });
});

describe("SoundChoicePicker の音色・サンプルの挿入(Issue #500)", () => {
  it("音色ごとに、番号と名前の分かる「挿入」ボタンがある", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    for (const p of TIMBRE_LIST) {
      expect(getByTestId(`mml-timbre-${p.name}`).getAttribute("aria-label")).toBe(`@${p.number} ${p.name} を挿入`);
    }
  });

  it("音色のボタンを押すと、カーソル位置に @名前 が入る", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const view = await viewOf(findByTestId);
    view.dispatch({ selection: { anchor: 2 } });
    await fireEvent.click(getByTestId("mml-timbre-bell"));
    expect(view.state.doc.toString()).toBe("c4 @bell ");
  });

  it("サンプルのボタンが、サンプルの数だけある", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    for (const s of MML_SAMPLES) expect(getByTestId(`mml-sample-${s.id}`).textContent).toContain(s.label);
  });

  it("入力が空なら、確認なしでサンプルが入る", async () => {
    const { findByTestId, getByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:" });
    const view = await viewOf(findByTestId);
    await fireEvent.click(getByTestId("mml-sample-jingle"));
    await vi.waitFor(() => expect(view.state.doc.toString()).toBe(MML_SAMPLES.find((s) => s.id === "jingle")!.mml));
    expect(queryByTestId("mml-sample-confirm")).toBeNull();
  });

  it("空白だけの入力も、空として扱う", async () => {
    const { findByTestId, getByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml: \n " });
    const view = await viewOf(findByTestId);
    await fireEvent.click(getByTestId("mml-sample-two-tracks"));
    await vi.waitFor(() => expect(view.state.doc.toString()).toBe(MML_SAMPLES.find((s) => s.id === "two-tracks")!.mml));
    expect(queryByTestId("mml-sample-confirm")).toBeNull();
  });

  it("入力があるときは、確認が出て、まだ置き換えない", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const view = await viewOf(findByTestId);
    await fireEvent.click(getByTestId("mml-sample-jingle"));
    expect(getByTestId("mml-sample-confirm").textContent).toContain("置き換え");
    expect(view.state.doc.toString()).toBe("c4");
  });

  it("確認で「置き換える」を押すと、サンプルになり、確認は消える", async () => {
    const { findByTestId, getByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const view = await viewOf(findByTestId);
    await fireEvent.click(getByTestId("mml-sample-jingle"));
    await fireEvent.click(getByTestId("mml-sample-confirm-ok"));
    await vi.waitFor(() => expect(view.state.doc.toString()).toBe(MML_SAMPLES.find((s) => s.id === "jingle")!.mml));
    expect(queryByTestId("mml-sample-confirm")).toBeNull();
  });

  it("確認で「やめる」を押すと、入力は変わらず、確認は消える", async () => {
    const { findByTestId, getByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const view = await viewOf(findByTestId);
    await fireEvent.click(getByTestId("mml-sample-jingle"));
    await fireEvent.click(getByTestId("mml-sample-confirm-cancel"));
    expect(view.state.doc.toString()).toBe("c4");
    expect(queryByTestId("mml-sample-confirm")).toBeNull();
  });

  it("確認を出したまま入力を編集したら、確認は引っ込める(古い確認で上書きしない)", async () => {
    const { findByTestId, getByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const view = await viewOf(findByTestId);
    await fireEvent.click(getByTestId("mml-sample-jingle"));
    view.dispatch({ changes: { from: 2, insert: " d4" } });
    await vi.waitFor(() => expect(queryByTestId("mml-sample-confirm")).toBeNull());
    expect(view.state.doc.toString()).toBe("c4 d4");
  });
});

describe("SoundChoicePicker のエラー表示(Issue #500)", () => {
  const invalid = { status: "invalid", error: { position: 3, message: "不明な文字 `x` です" } } as const;

  it("エラー位置に下線が付き、aria-invalid になる", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue(invalid);
    const { container, findByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    await viewOf(findByTestId);
    await vi.waitFor(() => expect(container.querySelector(".mml-error")?.textContent).toBe("x"));
    expect(container.querySelector(".cm-content")?.getAttribute("aria-invalid")).toBe("true");
  });

  it("「N 文字目」を押すと、その位置にカーソルが移る", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue(invalid);
    const { findByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    const view = await viewOf(findByTestId);
    const jump = await findByTestId("mml-error-jump");
    expect(jump.textContent).toBe("4 文字目");
    await fireEvent.click(jump);
    expect(view.state.selection.main.head).toBe(3);
  });

  it("エラー表示のまま編集したら、次の解析結果を待たずに古い下線を消す", async () => {
    const parse = vi.spyOn(commands, "parseMml");
    parse.mockResolvedValueOnce(invalid);
    parse.mockReturnValue(new Promise<never>(() => {})); // 2 回目以降は返ってこない(解析中のまま)
    const { container, findByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    const view = await viewOf(findByTestId);
    await vi.waitFor(() => expect(container.querySelector(".mml-error")).not.toBeNull());
    setDoc(view, "c4 xy");
    await vi.waitFor(() => expect(container.querySelector(".mml-error")).toBeNull());
  });

  it("同じ画面に 2 つあっても、aria-describedby の id が衝突しない", async () => {
    const a = render(SoundChoicePicker, { value: "mml:c4" });
    const b = render(SoundChoicePicker, { value: "mml:d4" });
    // render の findBy* は document.body 全体を探すので、2 つ並べるときはコンテナ内に限る
    await viewOf(within(a.container).findByTestId);
    await viewOf(within(b.container).findByTestId);
    const idsA = a.container.querySelector(".cm-content")!.getAttribute("aria-describedby")!.split(" ");
    const idsB = b.container.querySelector(".cm-content")!.getAttribute("aria-describedby")!.split(" ");
    expect(idsA).toHaveLength(2);
    for (const id of idsA) expect(idsB).not.toContain(id);
    for (const id of idsA) expect(a.container.querySelector(`#${CSS.escape(id)}`)).not.toBeNull();
  });
});

describe("SoundChoicePicker の使用量(Issue #500)", () => {
  it("文字数・音数・トラック数・再生時間を上限と並べて出し、9 割以上は near にする", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue({
      status: "parsed",
      score: { notes: new Array(2700).fill(null), totalSec: 12.34, trackCount: 2 } as never,
    });
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4;d4" });
    await vi.waitFor(() => expect(getByTestId("mml-usage-notes").textContent).toBe("音 2700/3000"));
    expect(getByTestId("mml-usage-chars").textContent).toBe("文字 5/5000");
    expect(getByTestId("mml-usage-tracks").textContent).toBe("トラック 2/4");
    expect(getByTestId("mml-usage-sec").textContent).toBe("再生 12.3/90 秒");
    expect(getByTestId("mml-usage-notes").getAttribute("data-level")).toBe("near");
    expect(getByTestId("mml-usage-chars").getAttribute("data-level")).toBe("normal");
  });

  it("構文エラーのときは、音数と再生時間を — にする(文字数とトラック数は出す)", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue({
      status: "invalid",
      error: { position: 3, message: "不明な文字 `x` です" },
    });
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("文字目"));
    expect(getByTestId("mml-usage-notes").textContent).toBe("音 —/3000");
    expect(getByTestId("mml-usage-sec").textContent).toBe("再生 —/90 秒");
    expect(getByTestId("mml-usage-chars").textContent).toBe("文字 4/5000");
  });

  it("5000 文字を超えたら chars が over になる", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:" + "c".repeat(5001) });
    expect(getByTestId("mml-usage-chars").getAttribute("data-level")).toBe("over");
  });

  it("解析中は前回の音数と再生時間を保つ(300ms ごとに — へ点滅させない)", async () => {
    const parse = vi.spyOn(commands, "parseMml");
    parse.mockResolvedValueOnce({
      status: "parsed",
      score: { notes: new Array(10).fill(null), totalSec: 3, trackCount: 1 } as never,
    });
    parse.mockReturnValue(new Promise<never>(() => {})); // 2 回目以降は解析中のまま
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await vi.waitFor(() => expect(getByTestId("mml-usage-notes").textContent).toBe("音 10/3000"));
    setDoc(await viewOf(findByTestId), "c4d4");
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("確認中"));
    expect(getByTestId("mml-usage-notes").textContent).toBe("音 10/3000");
    expect(getByTestId("mml-usage-chars").textContent).toBe("文字 4/5000");
  });
});
