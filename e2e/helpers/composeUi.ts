// 作成欄(ComposeBar)を操作する E2E の共有 helper(Issue #641 / #664)。
// 予約投稿の helper(scheduledUi.ts)と、ノートを投稿するスペックの両方が使う。

import { debugLog, debugLogPath } from "./debugLog";

/**
 * 要素の矩形が `quietMs` のあいだ変わらなくなるまで待つ。
 *
 * 作成欄の textarea は、フォーカスと入力の有無で最小の高さが 34px ↔ 80px に変わり(`ComposeBar.svelte` の
 * `compact`)、0.12 秒かけて遷移する。その間はツールバーのボタンが縦に約 46px 動く。`waitForClickable` は
 * 「表示されていて有効」しか見ないので、遷移の途中で通ったあとのクリックは、動く前の座標に飛んで空振りする
 * (Issue #641。入力後の「予約」が textarea の上に落ちる、予約成功直後のトグルが空白に落ちる)。
 *
 * `browser.execute` の中には名前付きの関数を作らない(下の `dumpUiFailure` の注意を参照)。
 */
export async function waitForStableRect(selector: string, quietMs = 300): Promise<void> {
  let last = "";
  let since = Date.now();
  try {
    await browser.waitUntil(
      async () => {
        const rect = await browser.execute((sel: string) => {
          const r = document.querySelector(sel)?.getBoundingClientRect();
          return r ? `${r.x},${r.y},${r.width},${r.height}` : "";
        }, selector);
        if (rect === "" || rect !== last) {
          last = rect;
          since = Date.now();
          return false;
        }
        return Date.now() - since >= quietMs;
      },
      { timeout: 15000, interval: 50 },
    );
  } catch (e) {
    // timeoutMsg は呼び出し時に評価されて last が空のままなので、ここで組み立てる
    throw new Error(`layout of ${selector} did not settle within 15s (last rect: ${last || "element not found"})\n${String(e)}`);
  }
}

/**
 * 作成欄の textarea の高さの遷移を止め、最後にクリックを受けた要素を記録する(Issue #690)。何度呼んでもよい。
 *
 * `waitForStableRect` だけでは足りない。CI では、本文の入力後 300ms 以上、ボタンが展開前の座標のまま測れて
 * 「落ち着いた」と判定され、`elementClick` の処理中に展開後の座標へ動いて、クリックが textarea に落ちた
 * (0.12 秒の遷移が進んでいなかった。描画更新の停滞とみられるが未確認)。遷移を無効にすれば、クラスが変わった
 * 時点で最終的なレイアウトに決まり、描画更新を待たない。進行中の遷移もその場で打ち切られる。
 */
async function prepareComposeForClick(): Promise<void> {
  await browser.execute(() => {
    if (document.getElementById("e2e-compose-no-transition")) return;
    const style = document.createElement("style");
    style.id = "e2e-compose-no-transition";
    style.textContent = '[data-testid="compose-textarea"] { transition: none !important; }';
    document.head.appendChild(style);
    document.addEventListener(
      "click",
      (e) => {
        const el = e.target as Element;
        document.documentElement.dataset.e2eLastClick = el.closest("[data-testid]")?.getAttribute("data-testid") ?? el.tagName;
      },
      true,
    );
  });
}

/** 要素が押せて、位置が落ち着いてからクリックする(遷移中の座標ずれによる空振りを避ける)。 */
export async function clickStable(testid: string): Promise<void> {
  const selector = `[data-testid="${testid}"]`;
  await prepareComposeForClick();
  await (await $(selector)).waitForClickable({ timeout: 15000 });
  await waitForStableRect(selector);
  await (await $(selector)).click();
}

/**
 * 作成欄・予約 UI の操作が失敗したとき、アプリが出したエラー(`role="dialog"` のモーダル本文)と作成欄・
 * 予約欄・ボタンの状態を集め、スクリーンショットを `e2e/wdio-logs/` に残す(Issue #641 / #664。CI は
 * アプリ自身のログを回収しないため、画面の状態が無いと原因を追えない)。診断の失敗で本来のエラーを隠さないよう、ここでは
 * 例外を投げない。
 *
 * `browser.execute` に渡す関数の中で、名前付きの関数(`const f = () => ...` や `function f() {}`)を作らない
 * こと。E2E の TS 変換(tsx/esbuild の keepNames)がそれを `__name(...)` で包み、WebView 側には
 * `__name` が無いので `Can't find variable: __name` で診断ごと失敗する(2026-11-16 の CI で実際に起きた)。
 * `.map((x) => ...)` のように引数へ直接渡す無名関数は包まれない。
 */
export async function dumpUiFailure(label: string, extra: Record<string, unknown> = {}): Promise<string> {
  let detail = "";
  try {
    const state = await browser.execute(() => {
      const textarea = document.querySelector('[data-testid="compose-textarea"]') as HTMLTextAreaElement | null;
      const input = document.querySelector('[data-testid="compose-schedule-input"]') as HTMLInputElement | null;
      const toggle = document.querySelector('[data-testid="compose-schedule-toggle"]') as HTMLButtonElement | null;
      const submit = document.querySelector('[data-testid="compose-submit"]') as HTMLButtonElement | null;
      return {
        dialog: document.querySelector('[role="dialog"]')?.textContent?.trim() ?? null,
        textareaLength: textarea?.value.length ?? null,
        scheduleInputPresent: input !== null,
        scheduleInputRects: input?.getClientRects().length ?? null,
        scheduleInput: input?.value ?? null,
        toggleDisabled: toggle?.disabled ?? null,
        // 送信ボタンの表示が「…」なら投稿リクエストが戻っていない(クリックは届いている)。「投稿」のまま
        // 作成欄に文章が残っていれば、クリックが届いていない(Issue #664)。
        submitLabel: submit?.textContent?.trim() ?? null,
        submitDisabled: submit?.disabled ?? null,
        noteTextCount: document.querySelectorAll('[data-testid="note-text"]').length,
        // クリックの空振り(レイアウトの遷移中のクリック)を疑うときに、ボタンの位置を見る(Issue #641)
        toggleY: toggle ? Math.round(toggle.getBoundingClientRect().y) : null,
        submitY: submit ? Math.round(submit.getBoundingClientRect().y) : null,
        textareaHeight: textarea ? Math.round(textarea.getBoundingClientRect().height) : null,
        localHint: document.querySelector('[data-testid="compose-schedule-local-hint"]') !== null,
        // 最後にクリックを受けた要素の testid(Issue #690)。押したはずのボタンと違えば、クリックの空振り
        lastClick: document.documentElement.dataset.e2eLastClick ?? null,
        activeElement: document.activeElement?.getAttribute("data-testid") ?? document.activeElement?.tagName ?? null,
        browserNow: new Date().toISOString(),
      };
    });
    detail = `diagnostics: ${JSON.stringify({ ...state, ...extra })}`;
  } catch (err) {
    detail = `diagnostics unavailable: ${String(err)}`;
  }
  debugLog(label, `failed: ${detail}`);
  try {
    const safe = label.replace(/[^a-zA-Z0-9_-]+/g, "_");
    const path = debugLogPath(`ui-failure-${safe}-${Date.now()}.png`);
    await browser.saveScreenshot(path);
    debugLog(label, `saved screenshot: ${path}`);
  } catch (err) {
    debugLog(label, `failed to save screenshot: ${String(err)}`);
  }
  return detail;
}

/**
 * 作成欄に本文を入れて「投稿」を押し、作成欄が空になる(=送信が済む)まで待つ。
 *
 * `setValue` で textarea が伸びてボタンが動く最中にクリックすると空振りする(`waitForStableRect` を参照。
 * Issue #664)。クリックが届いたかどうかをここで確定させ、失敗したら画面の状態を残す。これにより、
 * 後続の「ノートが現れない」失敗が、クリックの空振りなのか、配信の遅れなのかを切り分けられる。
 */
export async function postNote(text: string): Promise<void> {
  const textarea = await $('[data-testid="compose-textarea"]');
  await textarea.waitForDisplayed({ timeout: 15000 });
  await textarea.setValue(text);
  await clickStable("compose-submit");
  try {
    await browser.waitUntil(async () => (await (await $('[data-testid="compose-textarea"]')).getValue()) === "", {
      timeout: 20000,
      interval: 300,
    });
  } catch (e) {
    const detail = await dumpUiFailure(`postNote-${text}`);
    throw new Error(`compose box did not clear after posting "${text}"\n${detail}\n${String(e)}`);
  }
}

/**
 * 本文に `text` を含むノートが現れるまで待つ。`scope` を渡すと、その要素の中だけを見る。
 * 現れなければ、画面の状態を残して失敗する(Issue #664)。
 *
 * `*=text` セレクタ + `waitForDisplayed` は、DOM にあるのに `false` のままになることがあったため、
 * `note-text` の本文を直接ポーリングする。
 */
export async function waitForNoteText(text: string, scope?: WebdriverIO.Element, timeout = 20000): Promise<void> {
  try {
    await browser.waitUntil(
      async () => {
        const els = await (scope ? scope.$$('[data-testid="note-text"]') : $$('[data-testid="note-text"]'));
        for (const el of els) {
          const t = await el.getText().catch(() => "");
          if (t.includes(text)) return true;
        }
        return false;
      },
      { timeout, interval: 300 },
    );
  } catch (e) {
    const detail = await dumpUiFailure(`waitForNoteText-${text}`);
    throw new Error(`note "${text}" did not appear${scope ? " in the given column" : ""}\n${detail}\n${String(e)}`);
  }
}
