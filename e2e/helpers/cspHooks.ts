// 各テストの後に、アプリが CSP 違反を起こしていないことを確かめる(Issue #612)。
// wdio の afterTest は例外を投げてもテストが失敗しないので、mocha のルートフックにする。
// 記録は WebView の読み込み直し(設定のインポートなど)で消えるため、それより前の違反は拾えない。
const SESSION_GONE = /invalid session id|no such window|session (is )?(deleted|terminated|not (created|found))/i;

export const mochaHooks = {
  async afterEach(): Promise<void> {
    let violations: unknown[] | null = null;
    try {
      // 読み込み直しの直後は main.ts がまだ動いていないことがあるので、少し待つ。
      await browser.waitUntil(
        async () => {
          violations = await browser.execute(
            () => (window as unknown as { __cspViolations?: unknown[] }).__cspViolations ?? null,
          );
          return violations !== null;
        },
        { timeout: 5000, timeoutMsg: "CSP violation recorder is not installed" },
      );
    } catch (err) {
      // テストがアプリやセッションを終わらせた場合は確かめようがないので、警告だけにする。
      if (SESSION_GONE.test(String(err))) {
        console.warn(`[csp] skipped the violation check: ${String(err)}`);
        return;
      }
      throw err;
    }
    const found = violations as unknown[] | null;
    if (found && found.length > 0) throw new Error(`CSP violations: ${JSON.stringify(found)}`);
  },
};
