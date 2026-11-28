// CSP が実際に適用されていることを確かめる(Issue #612)。違反 0 件だけでは、CSP の無いビルドでも通ってしまう。
type CspWindow = { __cspViolations?: unknown[]; __cspProbeRan?: boolean };

describe("CSP", () => {
  it("インラインスクリプトを実行しない", async () => {
    await browser.waitUntil(async () =>
      browser.execute(() => Array.isArray((window as unknown as CspWindow).__cspViolations)),
    );
    // 起動時の違反を、この後の後始末でまとめて消してしまわないように、先に確かめる。
    const atBoot = await browser.execute(() => JSON.stringify((window as unknown as CspWindow).__cspViolations));
    if (atBoot !== "[]") throw new Error(`起動時に CSP 違反がある: ${atBoot}`);
    await browser.execute(() => {
      const s = document.createElement("script");
      s.textContent = "window.__cspProbeRan = true";
      document.head.appendChild(s);
    });
    await browser.waitUntil(
      async () => browser.execute(() => ((window as unknown as CspWindow).__cspViolations?.length ?? 0) > 0),
      { timeout: 10000, timeoutMsg: "CSP 違反が記録されない(CSP が適用されていない)" },
    );
    const ran = await browser.execute(() => (window as unknown as CspWindow).__cspProbeRan === true);
    if (ran) throw new Error("インラインスクリプトが実行された");
    // 自分で起こした違反を消す(残すとルートフックが失敗にする)。
    await browser.execute(() => {
      (window as unknown as { __cspViolations: unknown[] }).__cspViolations.length = 0;
    });
  });
});
