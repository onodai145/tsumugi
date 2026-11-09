import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: Column } = await import("./Column.svelte");
const { app } = await import("../lib/store.svelte");
import type { GroupView, TabView } from "../lib/store.svelte";

function tab(id: string): TabView {
  return {
    id,
    accountId: "a1",
    kind: { type: "home" },
    title: "ホーム",
    customTitle: id,
    filter: { kind: "none" },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes: [],
    notifications: [],
    state: "connected",
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    epoch: 0,
    selectedNoteId: null,
    selectionMoveSeq: 0,
  } as unknown as TabView;
}

const group: GroupView = { id: "g1", width: 300, auto: false, tabs: [tab("t1"), tab("t2")], activeTabId: "t1" };

function renderColumn() {
  return render(Column, {
    group,
    onAddTab: vi.fn(),
    onEditTab: vi.fn(),
    onEditGroup: vi.fn(),
    onSplitDown: vi.fn(),
    onSplitRight: vi.fn(),
  });
}

/// タブバー(`data-testid="column-tabbar"`)とタブ本文領域(カラム直下の `.min-h-0.flex-1`)を返す。
function layout(container: HTMLElement) {
  const bar = container.querySelector<HTMLElement>('[data-testid="column-tabbar"]');
  const section = container.querySelector<HTMLElement>("section.column-root");
  const body = section?.querySelector<HTMLElement>(":scope > .min-h-0.flex-1");
  return { bar, body };
}

afterEach(() => {
  cleanup();
  app.ui = { ...app.ui, tabBarPosition: "top", uiMode: "auto" };
});

describe("Column タブバーの位置(Issue #577)", () => {
  it("既定(top)ではタブバーが本文より前にあり、区切り線は下側・アクセント線は上端に付く", () => {
    const { container } = renderColumn();
    const { bar, body } = layout(container);
    expect(bar).not.toBeNull();
    expect(body).not.toBeNull();
    expect(bar!.compareDocumentPosition(body!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(bar!.classList.contains("border-b")).toBe(true);
    expect(bar!.classList.contains("border-t-2")).toBe(true);
    expect(bar!.classList.contains("border-t")).toBe(false);
  });

  it("bottomではタブバーが本文より後ろにあり、区切り線は上側・アクセント線は下端に付く", () => {
    app.ui = { ...app.ui, tabBarPosition: "bottom" };
    const { container } = renderColumn();
    const { bar, body } = layout(container);
    expect(bar).not.toBeNull();
    expect(body).not.toBeNull();
    expect(body!.compareDocumentPosition(bar!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(bar!.classList.contains("border-t")).toBe(true);
    expect(bar!.classList.contains("border-b-2")).toBe(true);
    expect(bar!.classList.contains("border-b")).toBe(false);
  });

  it("未知の値はtop扱いになる", () => {
    app.ui = { ...app.ui, tabBarPosition: "left" };
    const { container } = renderColumn();
    const { bar, body } = layout(container);
    expect(bar!.compareDocumentPosition(body!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("アクティブタブの強調線は、topでは下側・bottomでは上側(本文と反対側)に付く", () => {
    const top = renderColumn();
    const topActive = top.container.querySelector<HTMLElement>('[data-tab-id="t1"]')!;
    expect(topActive.className).toContain("inset_0_-2px");
    cleanup();

    app.ui = { ...app.ui, tabBarPosition: "bottom" };
    const bottom = renderColumn();
    const bottomActive = bottom.container.querySelector<HTMLElement>('[data-tab-id="t1"]')!;
    expect(bottomActive.className).toContain("inset_0_2px");
    expect(bottomActive.className).not.toContain("inset_0_-2px");
  });

  it("メニューは、topではボタンの下に、bottomではボタンの上に開く", async () => {
    const top = renderColumn();
    await fireEvent.click(top.getByTitle("メニュー"));
    expect(top.getByRole("menu").getAttribute("style")).toMatch(/top:/);
    expect(top.getByRole("menu").getAttribute("style")).not.toMatch(/bottom:/);
    cleanup();

    app.ui = { ...app.ui, tabBarPosition: "bottom" };
    const bottom = renderColumn();
    await fireEvent.click(bottom.getByTitle("メニュー"));
    expect(bottom.getByRole("menu").getAttribute("style")).toMatch(/bottom:/);
    expect(bottom.getByRole("menu").getAttribute("style")).not.toMatch(/top:/);
  });
});
