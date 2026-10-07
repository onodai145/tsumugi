import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, fireEvent, waitFor, screen } from "@testing-library/svelte";
import { app } from "../lib/store.svelte";
import { epochSecToLocalInput, localInputToEpochSec } from "../lib/schedule";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom)で import が失敗しないようスタブする(NoteCard.test.tsと同じパターン)。
vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

// __TAURI_INVOKE(生成bindings内でのinvoke呼び出し)は素の値を返す(typedError()側で
// { status: "ok", data } に包まれる)。ここで { status, data } を返してしまうと二重に
// 包まれてしまい unwrapAcc() 側の判定が壊れるため、コマンドごとの「生の戻り値」を返す。
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const { default: ComposeBar } = await import("./ComposeBar.svelte");

function setupAccount() {
  app.accounts = [
    {
      id: "acc1",
      host: "misskey.io",
      username: "me",
      userId: "u1",
      displayName: "Me",
      avatarUrl: null,
      instance: null,
    },
  ];
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_drafts") return Promise.resolve([]);
    if (cmd === "get_auto_draft") return Promise.resolve(null);
    return Promise.resolve(null);
  });
  setupAccount();
  // ComposeBarの自動復元effectはapp.booting===falseになるまで待つ(App.svelteの
  // app.boot()完了を模す)。ComposeBar単体テストではboot()自体は呼ばず、フラグだけ倒す。
  app.booting = false;
});

afterEach(() => {
  cleanup();
  app.accounts = [];
  app.booting = true;
});

describe("ComposeBar 下書き", () => {
  it("マウント時にget_auto_draftを呼ぶ", async () => {
    render(ComposeBar);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_auto_draft", { accountId: "acc1" });
    });
  });

  it("入力後2秒でsave_auto_draftを呼ぶ", async () => {
    vi.useFakeTimers();
    try {
      const { getByTestId } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "書きかけ" } });
      // 2000msのデバウンスであること自体を検証する: 直前(1900ms)ではまだ呼ばれておらず、
      // 2000msに達した時点で初めて呼ばれることを確認する(単に「いつかは呼ばれる」だけでは
      // デバウンスの有無を区別できないため)。
      await vi.advanceTimersByTimeAsync(1900);
      expect(invokeMock).not.toHaveBeenCalledWith("save_auto_draft", expect.anything());
      await vi.advanceTimersByTimeAsync(100);
      expect(invokeMock).toHaveBeenCalledWith(
        "save_auto_draft",
        expect.objectContaining({ accountId: "acc1" }),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("空に戻すとclear_auto_draftを呼ぶ", async () => {
    const { getByTestId } = render(ComposeBar);
    // マウント直後(自動復元完了直後)の時点では、まだ何も入力していないので
    // clear_auto_draftが呼ばれてはならない(復元しようとしている自動下書きを誤って
    // 消してしまう回帰: 自動保存effectと自動復元effectの宣言順序に依存するハザードへの
    // リグレッションガード)。
    expect(invokeMock).not.toHaveBeenCalledWith("clear_auto_draft", expect.anything());
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "a" } });
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "" } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("clear_auto_draft", { accountId: "acc1" });
    });
  });

  it("デバウンスが既に発火済みの場合、アンマウント時にsave_auto_draftを再送しない", async () => {
    vi.useFakeTimers();
    try {
      const { getByTestId, unmount } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "デバウンス完了済み" } });
      await vi.advanceTimersByTimeAsync(2000); // デバウンスを確定させる(save_auto_draftが1回発火)
      const saveCallsBeforeUnmount = invokeMock.mock.calls.filter((c) => c[0] === "save_auto_draft").length;
      expect(saveCallsBeforeUnmount).toBe(1);
      unmount();
      const saveCallsAfterUnmount = invokeMock.mock.calls.filter((c) => c[0] === "save_auto_draft").length;
      expect(saveCallsAfterUnmount).toBe(1); // 発火済みタイマーをonDestroyが誤って再送しない
    } finally {
      vi.useRealTimers();
    }
  });

  it("投稿処理中にデバウンスが発火してもsave_auto_draftを送らない(Issue #303: 投稿完了後のclear_auto_draftより後に反映され下書きが残留するのを防ぐ)", async () => {
    vi.useFakeTimers();
    let resolvePostNote!: (value: unknown) => void;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "get_auto_draft") return Promise.resolve(null);
      if (cmd === "post_note") {
        // モバイルの低速回線などでpost_noteの往復が2秒デバウンスより長くかかる状況を模す。
        return new Promise((resolve) => {
          resolvePostNote = resolve;
        });
      }
      return Promise.resolve(null);
    });
    try {
      const { getByTestId } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "投稿する本文" } });
      // デバウンスが確定する(2000ms)前に投稿を開始する
      await vi.advanceTimersByTimeAsync(500);
      await fireEvent.click(getByTestId("compose-submit"));
      // post_noteがまだ解決していない間にデバウンスの残り時間が経過しても、
      // save_auto_draftが発火してはならない(発火するとclear_auto_draftより後に
      // 届いて下書きが残留しうる)。
      await vi.advanceTimersByTimeAsync(2000);
      expect(invokeMock).not.toHaveBeenCalledWith("save_auto_draft", expect.anything());
      resolvePostNote({ id: "n1" });
      await vi.advanceTimersByTimeAsync(0);
      expect(invokeMock).toHaveBeenCalledWith("clear_auto_draft", { accountId: "acc1" });
    } finally {
      vi.useRealTimers();
    }
  });

  it("添付アップロード完了で自動保存effectが再武装しても投稿処理中はsave_auto_draftを送らない(Issue #303)", async () => {
    vi.useFakeTimers();
    let resolvePostNote!: (value: unknown) => void;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "get_auto_draft") return Promise.resolve(null);
      if (cmd === "upload_file") {
        return Promise.resolve({
          id: "f1",
          name: "doc.txt",
          mimeType: "text/plain",
          size: 1,
          url: "https://misskey.io/files/f1",
          thumbnailUrl: null,
          isSensitive: false,
          comment: null,
        });
      }
      if (cmd === "post_note") {
        // 添付アップロード完了(=attachments更新)で自動保存effectが再武装した後、
        // post_note自体の往復がデバウンスより長くかかる状況を模す。
        return new Promise((resolve) => {
          resolvePostNote = resolve;
        });
      }
      return Promise.resolve(null);
    });
    try {
      render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      app.openCompose("acc1", { text: "写真の説明", filePaths: ["/tmp/doc.txt"] });
      await vi.advanceTimersByTimeAsync(0); // addLocalAttachmentの非同期処理を消化する
      await waitFor(() => {
        expect(screen.getByDisplayValue("写真の説明")).toBeTruthy();
      });
      await fireEvent.click(screen.getByTestId("compose-submit"));
      await vi.advanceTimersByTimeAsync(0); // アップロード(upload_file)の解決とattachments更新を消化する
      // post_noteがまだ解決していない間に、再武装されたデバウンスの2000msが経過しても
      // save_auto_draftが発火してはならない(発火するとclear_auto_draftより後に届いて
      // 下書きが残留しうる)。
      await vi.advanceTimersByTimeAsync(2000);
      expect(invokeMock).not.toHaveBeenCalledWith("save_auto_draft", expect.anything());
      resolvePostNote({ id: "n1" });
      await vi.advanceTimersByTimeAsync(0);
      expect(invokeMock).toHaveBeenCalledWith("clear_auto_draft", { accountId: "acc1" });
    } finally {
      vi.useRealTimers();
    }
  });

  it("投稿失敗後は自動保存が再武装され下書きが保持される(Issue #303のbusyガードが失敗時の復帰を妨げないことの確認)", async () => {
    vi.useFakeTimers();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "get_auto_draft") return Promise.resolve(null);
      if (cmd === "post_note") return Promise.reject(new Error("network error"));
      return Promise.resolve(null);
    });
    try {
      const { getByTestId } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "失敗する投稿" } });
      await vi.advanceTimersByTimeAsync(500);
      await fireEvent.click(getByTestId("compose-submit"));
      await vi.advanceTimersByTimeAsync(0); // post_noteの拒否とcatch節を消化する
      await vi.advanceTimersByTimeAsync(2000);
      expect(invokeMock).toHaveBeenCalledWith(
        "save_auto_draft",
        expect.objectContaining({ accountId: "acc1" }),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("デバウンス確定前にアンマウントされてもsave_auto_draftがflushされる", async () => {
    vi.useFakeTimers();
    try {
      const { getByTestId, unmount } = render(ComposeBar);
      await vi.advanceTimersByTimeAsync(0); // マウント時のget_auto_draft(非同期)を先に消化する
      await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "閉じる直前の入力" } });
      // 2000msのデバウンスが確定する前(モバイル投稿モーダルを閉じる操作を模す)にアンマウントする
      await vi.advanceTimersByTimeAsync(500);
      expect(invokeMock).not.toHaveBeenCalledWith("save_auto_draft", expect.anything());
      unmount();
      expect(invokeMock).toHaveBeenCalledWith(
        "save_auto_draft",
        expect.objectContaining({ accountId: "acc1" }),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("手動下書きを呼び出すとtextが復元され、投稿成功後にdelete_draftが呼ばれる", async () => {
    const draft = {
      id: "d1",
      accountId: "acc1",
      kind: "manual",
      text: "保存済み本文",
      cw: null,
      visibility: "public",
      localOnly: false,
      reactionAcceptance: "all",
      channelId: null,
      poll: null,
      fileIds: [],
      replyNote: null,
      quoteNote: null,
      createdAt: 0,
      updatedAt: 0,
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([draft]);
      if (cmd === "get_auto_draft") return Promise.resolve(null);
      if (cmd === "post_note") return Promise.resolve({ id: "n1" });
      if (cmd === "delete_draft") return Promise.resolve(null);
      if (cmd === "clear_auto_draft") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    const { getByTitle, getByText, getByTestId } = render(ComposeBar);
    await fireEvent.click(getByTitle("下書き"));
    await waitFor(() => expect(getByText("保存済み本文")).toBeTruthy());
    await fireEvent.click(getByText("保存済み本文"));
    expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("保存済み本文");

    await fireEvent.click(getByTestId("compose-submit"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("delete_draft", { accountId: "acc1", draftId: "d1" });
    });
  });

  it("app.composeのtextとfilePathsをコンポーズ欄に反映する(Issue #116)", async () => {
    setupAccount();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "read_attachment_preview") return Promise.resolve("data:image/png;base64,xx");
      return Promise.resolve(null);
    });
    render(ComposeBar);
    app.openCompose("acc1", { text: "共有されたテキスト", filePaths: ["/tmp/shared-intents/photo.png"] });
    await waitFor(() => {
      expect(screen.getByDisplayValue("共有されたテキスト")).toBeTruthy();
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "read_attachment_preview",
        expect.objectContaining({ path: "/tmp/shared-intents/photo.png" }),
      );
    });
  });

  it("app.composeのtextは既存の入力があれば上書きしない(Issue #116)", async () => {
    setupAccount();
    invokeMock.mockResolvedValue(null);
    const { container } = render(ComposeBar);
    const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "書きかけの本文" } });
    app.openCompose("acc1", { text: "共有されたテキスト" });
    await waitFor(() => {
      expect(screen.getByDisplayValue("書きかけの本文")).toBeTruthy();
    });
  });

  it("共有添付の追加中に自動下書き復元が解決しても添付を消さない(Issue #116)", async () => {
    setupAccount();
    let resolveAutoDraft!: (value: unknown) => void;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "get_auto_draft") {
        // マウント時の自動復元effectをこの時点では確定させず、共有添付の反映と
        // 競合させる(Task 4レビューで見つかったレース: Issue #116)。
        return new Promise((resolve) => {
          resolveAutoDraft = resolve;
        });
      }
      return Promise.resolve(null);
    });
    render(ComposeBar);
    // テキストを伴わない共有(例: キャプション無しの画像共有)を模す。
    app.openCompose("acc1", { filePaths: ["/tmp/shared-intents/shared.txt"] });
    await waitFor(() => {
      expect(screen.getAllByTitle("削除")).toHaveLength(1);
    });
    // 添付の反映が終わった後に、保留中だった自動下書き復元が解決する。
    // ガードが無ければ loadDraft() が attachments = [] で上書きしてしまう。
    resolveAutoDraft({
      id: "d1",
      accountId: "acc1",
      kind: "auto",
      text: "自動保存されていた本文",
      cw: null,
      visibility: "public",
      localOnly: false,
      reactionAcceptance: "all",
      channelId: null,
      poll: null,
      fileIds: [],
      replyNote: null,
      quoteNote: null,
      createdAt: 0,
      updatedAt: 0,
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getAllByTitle("削除")).toHaveLength(1);
  });

  it("下書きのfileIds読み込み中に届いた共有添付をloadDraftが上書きしない(Issue #116 最終レビュー)", async () => {
    setupAccount();
    let resolveGetDriveFile!: (value: unknown) => void;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "get_auto_draft") {
        return Promise.resolve({
          id: "d1",
          accountId: "acc1",
          kind: "auto",
          text: "",
          cw: null,
          visibility: "public",
          localOnly: false,
          reactionAcceptance: "all",
          channelId: null,
          poll: null,
          fileIds: ["f1"],
          replyNote: null,
          quoteNote: null,
          createdAt: 0,
          updatedAt: 0,
        });
      }
      if (cmd === "get_drive_file") {
        // loadDraft()内のfileIds読み込みをここで足止めし、共有添付の追加(add_local_attachment
        // 相当のread_attachment_preview)と競合させる。
        return new Promise((resolve) => {
          resolveGetDriveFile = resolve;
        });
      }
      if (cmd === "read_attachment_preview") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    render(ComposeBar);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "get_drive_file",
        expect.objectContaining({ fileId: "f1" }),
      );
    });
    // fileIds読み込みが宙ぶらりんの間に共有(キャプション無し画像)が届く。
    app.openCompose("acc1", { filePaths: ["/tmp/shared-intents/photo.png"] });
    await waitFor(() => {
      expect(screen.getAllByTitle("削除")).toHaveLength(1);
    });
    // 保留中だったget_drive_fileが解決し、loadDraft()が添付一覧を確定させる。
    // ガードが無ければここで attachments = [下書きのファイルのみ] に上書きされ、
    // 共有添付が消える。
    resolveGetDriveFile({
      id: "f1",
      mimeType: "image/png",
      isSensitive: false,
      url: "https://example.com/f1.png",
      thumbnailUrl: null,
      name: "f1.png",
    });
    await waitFor(() => {
      expect(screen.getAllByTitle("削除")).toHaveLength(2);
    });
  });
});

describe("ComposeBar 貼り付け(Issue #117)", () => {
  function paste(textarea: HTMLElement, types: string[], plain = "") {
    return fireEvent.paste(textarea, { clipboardData: { types, getData: (t: string) => (t === "text/plain" ? plain : "") } });
  }

  // files に { kind, message } を渡すと IPC 失敗を模す。Tauri の実エラーはプレーンオブジェクトで
  // reject される(生成 bindings の typedError は Error インスタンスだけを再 throw するため、
  // new Error(...) を reject させると status: "error" にならない)。
  function mockClipboard(opts: { files?: string[] | { kind: string; message: string }; text?: string }) {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "read_clipboard_files") {
        return opts.files && !Array.isArray(opts.files) ? Promise.reject(opts.files) : Promise.resolve(opts.files ?? []);
      }
      if (cmd === "read_clipboard_image") return Promise.reject({ kind: "invalid", message: "no image" });
      if (cmd === "read_clipboard_text") return Promise.resolve(opts.text ?? "");
      if (cmd === "read_attachment_preview") return Promise.resolve("data:image/png;base64,xx");
      return Promise.resolve(null);
    });
  }

  const clipboardCalls = () =>
    invokeMock.mock.calls.map((c) => c[0] as string).filter((c) => c.startsWith("read_clipboard_"));

  it("ファイル参照を貼り付けると、複数ファイルが順序どおり添付される", async () => {
    mockClipboard({ files: ["/home/u/a b.mp4", "/home/u/日本語.png"] });
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), ["text/uri-list"]);
    await waitFor(() => expect(screen.getAllByTitle("削除")).toHaveLength(2));
    // 画像拡張子のファイルだけプレビューが読まれ、動画は拡張子バッジになる
    expect(invokeMock).toHaveBeenCalledWith("read_attachment_preview", expect.objectContaining({ path: "/home/u/日本語.png" }));
    expect(invokeMock).not.toHaveBeenCalledWith("read_attachment_preview", expect.objectContaining({ path: "/home/u/a b.mp4" }));
    expect(screen.getByText("MP4")).toBeTruthy();
    // ファイルが取れた場合は画像・テキストの読み取りに進まない
    expect(clipboardCalls()).toEqual(["read_clipboard_files"]);
  });

  it("通常のテキスト貼り付けは横取りせず、Rust への IPC も発生しない", async () => {
    mockClipboard({});
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), [], "hello");
    await Promise.resolve();
    expect(clipboardCalls()).toEqual([]);
  });

  it("ファイルが得られず text/uri-list だけ付いている場合(URL コピー等)はテキストを復元する", async () => {
    mockClipboard({ files: [], text: "https://example.com/" });
    // jsdom には execCommand("insertText") が無いため、呼び出し内容で検証する
    const exec = vi.fn().mockReturnValue(true);
    (document as unknown as { execCommand: unknown }).execCommand = exec;
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), ["text/uri-list"]);
    await waitFor(() => expect(exec).toHaveBeenCalledWith("insertText", false, "https://example.com/"));
    expect(clipboardCalls()).toEqual(["read_clipboard_files", "read_clipboard_image", "read_clipboard_text"]);
    expect(screen.queryAllByTitle("削除")).toHaveLength(0);
  });

  it("read_clipboard_files の IPC が失敗してもエラー表示せず画像・テキストへ進む", async () => {
    mockClipboard({ files: { kind: "network", message: "boom" }, text: "" });
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), ["text/uri-list"]);
    await waitFor(() => expect(clipboardCalls()).toEqual(["read_clipboard_files", "read_clipboard_image", "read_clipboard_text"]));
    expect(screen.queryByText(/boom/)).toBeNull();
  });

  it("本文が空でファイルも無ければ従来どおり画像を試す(#57)", async () => {
    mockClipboard({ files: [] });
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), []);
    await waitFor(() => expect(clipboardCalls()).toContain("read_clipboard_image"));
  });
});

describe("ComposeBar 予約投稿", () => {
  function mockCaps(available: boolean) {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "get_auto_draft") return Promise.resolve(null);
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available });
      if (cmd === "schedule_note") return Promise.resolve({ id: "d1" });
      return Promise.resolve(null);
    });
  }
  const futureInput = () => epochSecToLocalInput(Math.floor((Date.now() + 86_400_000) / 1000));

  it("非対応サーバーでは予約ボタンを出さない", async () => {
    mockCaps(false);
    const { queryByTestId } = render(ComposeBar);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_schedule_capabilities", { accountId: "acc1" }));
    expect(queryByTestId("compose-schedule-toggle")).toBeNull();
  });

  it("対応サーバーで日時を設定すると投稿ボタンが「予約」になり、schedule_note を呼ぶ(post_note は呼ばない)", async () => {
    mockCaps(true);
    const { findByTestId, getByTestId } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    const value = futureInput();
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value } });
    expect(getByTestId("compose-submit").textContent).toContain("予約");

    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "あとで投稿" } });
    await fireEvent.click(getByTestId("compose-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("schedule_note", {
        accountId: "acc1",
        draft: expect.objectContaining({ text: "あとで投稿" }),
        scheduledAt: localInputToEpochSec(value),
      }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("post_note", expect.anything());
    // 成功したら作成欄と予約日時がクリアされ、投稿ボタンも元に戻る
    await waitFor(() => expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe(""));
    expect(getByTestId("compose-submit").textContent).toContain("投稿");
  });

  // Review Focus 3: 入力後に時間が経って過去になった場合はサーバーに送らない
  it("過去の日時ではエラーを出し、schedule_note を呼ばない", async () => {
    mockCaps(true);
    const { findByTestId, getByTestId, findByText } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    const past = epochSecToLocalInput(Math.floor((Date.now() - 86_400_000) / 1000));
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value: past } });
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "x" } });
    await fireEvent.click(getByTestId("compose-submit"));
    expect(await findByText("予約日時は現在より後にしてください")).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith("schedule_note", expect.anything());
  });

  it("予約日時を解除すると通常の投稿に戻る", async () => {
    mockCaps(true);
    const { findByTestId, getByTestId } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value: futureInput() } });
    await fireEvent.click(getByTestId("compose-schedule-clear"));
    expect(getByTestId("compose-submit").textContent).toContain("投稿");
  });

  it("サーバーの上限エラーは日本語で表示する", async () => {
    mockCaps(true);
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: true });
      if (cmd === "schedule_note")
        return Promise.reject({ kind: "api", message: "notes/drafts/create: TOO_MANY_SCHEDULED_NOTES x" });
      return Promise.resolve(cmd === "list_drafts" ? [] : null);
    });
    const { findByTestId, getByTestId, findByText } = render(ComposeBar);
    await fireEvent.click(await findByTestId("compose-schedule-toggle"));
    await fireEvent.input(getByTestId("compose-schedule-input"), { target: { value: futureInput() } });
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "x" } });
    await fireEvent.click(getByTestId("compose-submit"));
    expect(await findByText(/予約できる投稿数の上限/)).toBeTruthy();
    // 失敗したので作成欄は残る
    expect((getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("x");
  });
  // render() の戻り値型は render<Component> が絞り込まれていて関数引数に渡せないため、使う分だけ構造的に型付けする
  type UiQueries = {
    container: HTMLElement;
    findByTestId(id: string): Promise<HTMLElement>;
    getByTestId(id: string): HTMLElement;
    getByText(text: string): HTMLElement;
    getByPlaceholderText(text: string): HTMLElement;
  };

  const scheduledNote = (over: Record<string, unknown> = {}) => ({
    id: "s1",
    scheduledAt: Math.floor(Date.now() / 1000) + 3600,
    text: "戻したい本文",
    cw: null,
    visibility: "home",
    localOnly: false,
    reactionAcceptance: "all",
    channelId: null,
    poll: null,
    fileIds: [],
    replyNote: null,
    quoteNote: null,
    ...over,
  });

  async function openScheduledList(ui: UiQueries) {
    await fireEvent.click(await ui.findByTestId("compose-schedule-toggle"));
    await fireEvent.click(ui.getByTestId("compose-scheduled-list"));
  }

  it("「作成欄に戻す」で内容を作成欄に読み込み、サーバー側の予約を削除する", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: true });
      if (cmd === "list_scheduled_notes") return Promise.resolve([scheduledNote()]);
      if (cmd === "list_drafts") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const ui = render(ComposeBar);
    await openScheduledList(ui);
    await fireEvent.click(await ui.findByTestId("scheduled-restore-s1"));

    await waitFor(() =>
      expect((ui.getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("戻したい本文"),
    );
    expect(invokeMock).toHaveBeenCalledWith("cancel_scheduled_note", { accountId: "acc1", draftId: "s1" });
    // モーダルは閉じる
    expect(ui.queryByTestId("scheduled-item-s1")).toBeNull();
  });

  // Review Focus 5: 削除だけ失敗しても作成欄の内容は残し、重複投稿の恐れを警告する
  it("戻した後にサーバー側の削除が失敗したら、内容は残して重複の警告を出す", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_schedule_capabilities") return Promise.resolve({ available: true });
      if (cmd === "list_scheduled_notes") return Promise.resolve([scheduledNote()]);
      if (cmd === "cancel_scheduled_note") return Promise.reject({ kind: "network", message: "offline" });
      if (cmd === "list_drafts") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const ui = render(ComposeBar);
    await openScheduledList(ui);
    await fireEvent.click(await ui.findByTestId("scheduled-restore-s1"));

    expect(await ui.findByText(/重複/)).toBeTruthy();
    expect((ui.getByTestId("compose-textarea") as HTMLTextAreaElement).value).toBe("戻したい本文");
  });

  // Review Focus 4: 期間指定の投票は、投稿時刻ではなく予約日時を基準に締切を計算する
  async function setupPoll(ui: UiQueries, scheduleValue: string) {
    await fireEvent.click(await ui.findByTestId("compose-schedule-toggle"));
    await fireEvent.input(ui.getByTestId("compose-schedule-input"), { target: { value: scheduleValue } });
    await fireEvent.input(ui.getByTestId("compose-textarea"), { target: { value: "投票つき" } });
    await fireEvent.click(ui.getByText("投票"));
    await fireEvent.input(ui.getByPlaceholderText("選択肢 1"), { target: { value: "A" } });
    await fireEvent.input(ui.getByPlaceholderText("選択肢 2"), { target: { value: "B" } });
  }

  it("期間指定の投票は予約日時を基準に締切を計算する", async () => {
    mockCaps(true);
    const ui = render(ComposeBar);
    const value = futureInput();
    await setupPoll(ui, value);
    await fireEvent.click(ui.getByText("期間を指定"));
    await fireEvent.input(ui.container.querySelector("input[type=number]") as HTMLInputElement, {
      target: { value: "2" },
    });
    // 単位は既定の「時間後」
    await fireEvent.click(ui.getByTestId("compose-submit"));

    const scheduledAt = localInputToEpochSec(value) as number;
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("schedule_note", {
        accountId: "acc1",
        draft: expect.objectContaining({
          poll: expect.objectContaining({ expiresAt: scheduledAt * 1000 + 2 * 3_600_000 }),
        }),
        scheduledAt,
      }),
    );
  });

  it("日時指定の投票の締切が予約日時以前ならエラーを出し、schedule_note を呼ばない", async () => {
    mockCaps(true);
    const ui = render(ComposeBar);
    const value = futureInput();
    await setupPoll(ui, value);
    await fireEvent.click(ui.getByText("日時を指定"));
    // 予約日時の1時間前を締切にする
    const before = epochSecToLocalInput((localInputToEpochSec(value) as number) - 3600);
    await fireEvent.input(ui.container.querySelector("input[type=datetime-local]:not([data-testid])") as HTMLInputElement, {
      target: { value: before },
    });
    await fireEvent.click(ui.getByTestId("compose-submit"));

    expect(await ui.findByText(/投票の締切/)).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith("schedule_note", expect.anything());
  });
});
