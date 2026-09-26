// カラム内容(ノート本文・URL・コードブロック・画像)による意図しない横はみ出しを検出する(Issue #383)。
// overflow.e2e.ts は [data-columns-scroll] の子孫を除外するため、カラム内は別ファイルのここで検査する。
// 各カラムのノート一覧は Column.svelte のslot(h-full w-full overflow-y-auto)で、overflow-yを指定すると
// overflow-xもautoになるため、中身が幅を超えると意図しない横スクロール(scrollWidth > clientWidth)として現れる。
// コードブロック等、自身でoverflowを持つ要素は slot の scrollWidth に影響しないので、意図したスクロールは除外される。
// 各ノート(article)自身の検査は必須(slotの検査だけでは冗長に見えても消さないこと): articleが content-visibility:auto で
// はみ出しをクリップすると slot の scrollWidth は増えず、slotだけの検査ではGREENのままになる。
// 検出力は本番を一時的に壊して確認済み(NoteCard の min-w-0 除去 / app.css の code-block を overflow: visible /
// MediaGrid のセルの overflow-hidden 除去)。LEN=200 は NoteCard の300文字折りたたみ閾値未満に収めるための値。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { createNote, signInAsSeededUser, uploadImage } from "../helpers/misskeyApi";
import { addAccountAndEnableMobile, addHomeColumn } from "../helpers/mobile";

// 先頭のslot(`> div`)を選ぶ。単一タブのカラム(Column.svelte の computeTabSlots が1 slotを返す)でのみ正しく、
// 複数タブのカラムでは先に prev slot を測ってしまう。
const SLOT = ".column-root .mobile-scroll-snap > div";
// NoteCard は本文が300文字(TEXT_COLLAPSE_THRESHOLD)を超えると折りたたみ(overflow-hidden)で
// はみ出しを隠してしまうため、折りたたまれない長さに収めつつカラム幅(~379px)を十分超える長さにする。
const LEN = 200;
const MARKERS = ["E2E-OVF-URL", "E2E-OVF-WORD", "E2E-OVF-CODE", "E2E-OVF-IMG"];

const measureSlot = () =>
  browser.execute((sel, MARKERS_IN) => {
    const slot = document.querySelector(sel) as HTMLElement | null;
    if (!slot) return null;
    const slotRect = slot.getBoundingClientRect();
    const offenders: string[] = [];
    for (const el of Array.from(slot.querySelectorAll("*"))) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      if (r.right > slotRect.right + 1) {
        offenders.push(`${el.tagName.toLowerCase()}.${String(el.getAttribute("class") ?? "").slice(0, 60)} right=${Math.round(r.right)} slotRight=${Math.round(slotRect.right)}`);
      }
    }
    // NoteCard の article は content-visibility:auto(=contain:paint)で、はみ出しを自身でクリップして
    // slot の scrollWidth には現れない(はみ出した内容は横スクロールではなく見切れとして現れる)。
    // そのため slot だけでなく各ノート(article)自身の scrollWidth > clientWidth も検査する。
    const clippedNotes = new Set<string>();
    for (const a of Array.from(slot.querySelectorAll("article"))) {
      if (a.scrollWidth - a.clientWidth > 1) {
        const marker = (a.textContent ?? "").match(/E2E-OVF-[A-Z]+/)?.[0] ?? "(unknown note)";
        clippedNotes.add(`${marker} article scrollWidth=${a.scrollWidth} clientWidth=${a.clientWidth}`);
      }
    }
    // 4件のマーカーノートが実際に測定され(found/height>0)、折りたたまれていない(collapsed=false)ことを返す。
    const markers = MARKERS_IN.map((m) => {
      const a = Array.from(slot.querySelectorAll("article")).find((x) => (x.textContent ?? "").includes(m));
      return {
        marker: m,
        found: !!a,
        height: a ? a.getBoundingClientRect().height : 0,
        collapsed: a ? !!a.querySelector(".note-text-collapsed") : false,
      };
    });
    return {
      markers,
      clippedNotes: Array.from(clippedNotes),
      scrollWidth: slot.scrollWidth,
      clientWidth: slot.clientWidth,
      slotRight: slotRect.right,
      vw: window.innerWidth,
      offenders: offenders.slice(0, 10),
    };
  }, SLOT, MARKERS);

// content-visibility:auto の画面外articleはプレースホルダ寸法になり得るため、各マーカーを表示域に入れ、高さが安定するまで待つ。
const settle = async () => {
  const heights = () =>
    browser.execute(
      (sel, ms) => {
        const slot = document.querySelector(sel)!;
        return ms.map((m) => {
          const a = Array.from(slot.querySelectorAll("article")).find((x) => (x.textContent ?? "").includes(m));
          if (!a) return -1;
          a.scrollIntoView({ block: "center" });
          return a.getBoundingClientRect().height;
        });
      },
      SLOT,
      MARKERS,
    );
  let prev = JSON.stringify(await heights());
  await browser.waitUntil(
    async () => {
      await browser.pause(300);
      const cur = JSON.stringify(await heights());
      const same = cur === prev;
      prev = cur;
      return same;
    },
    { timeout: 10000, interval: 100, timeoutMsg: "marker note heights did not stabilise" },
  );
};

describe("mobile horizontal overflow: column content", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    const token = await signInAsSeededUser();
    const fileId = await uploadImage(token);
    await createNote(token, `E2E-OVF-URL https://example.com/${"a".repeat(LEN)}`);
    await createNote(token, `E2E-OVF-WORD ${"W".repeat(LEN)}`);
    await createNote(token, `E2E-OVF-CODE\n\`\`\`\nconst x = '${"c".repeat(LEN)}';\n\`\`\``);
    await createNote(token, "E2E-OVF-IMG", [fileId]);

    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileOverflowColumn");
    await addHomeColumn();

    // ノートが描画される前に測ると空振りするため、4件すべてが描画されるまで待つ。
    await browser.waitUntil(
      async () => {
        const text = await browser.execute((sel) => (document.querySelector(sel) as HTMLElement | null)?.innerText ?? "", SLOT);
        return MARKERS.every((m) => text.includes(m));
      },
      { timeout: 30000, interval: 500, timeoutMsg: "posted notes did not render in the column" },
    );
  });

  after(async () => {
    await bridge?.teardown();
  });

  it("has no unintended horizontal scroll in the column note list", async () => {
    await settle();
    const m = await measureSlot();
    expect(m).not.toBeNull();
    const problems: string[] = [];
    for (const k of m!.markers) {
      if (!k.found) problems.push(`${k.marker}: not found`);
      else {
        if (!(k.height > 0)) problems.push(`${k.marker}: height=${k.height}`);
        if (k.collapsed) problems.push(`${k.marker}: collapsed`);
      }
    }
    expect(problems).toEqual([]);
    const over = m!.scrollWidth - m!.clientWidth > 1;
    // 失敗時に原因ノートを特定できるよう、幅を超えている要素/ノートの一覧をメッセージに含める。
    expect({ over, clippedNotes: m!.clippedNotes, offenders: over || m!.clippedNotes.length > 0 ? m!.offenders : [] }).toEqual({
      over: false,
      clippedNotes: [],
      offenders: [],
    });
  });

  it("keeps the column note list inside the viewport", async () => {
    const m = await measureSlot();
    expect(m).not.toBeNull();
    expect(m!.slotRight).toBeLessThanOrEqual(m!.vw + 1);
  });
});
