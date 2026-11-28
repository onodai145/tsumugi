// 各テストの後に、アプリが CSP 違反を起こしていないことを確かめる(Issue #612)。
// wdio の afterTest は例外を投げてもテストが失敗しないので、mocha のルートフックにする。mocha は afterEach が
// 失敗すると残りのテストを飛ばすので、afterEach では集めるだけにして、afterAll でまとめて失敗にする。
// 記録は WebView の読み込み直し(設定のインポートなど)で消えるため、それより前の違反は拾えない。

// アプリやセッションが終わった後に出る WebDriver のエラー。wdio の文言に依存するので、wdio を上げたときは確かめる。
const SESSION_GONE = /invalid session id|no such window|session (is )?(deleted|terminated|not (created|found))/i;

// この spec ファイルで見つかった問題。
const problems: string[] = [];

export const mochaHooks = {
  async afterEach(this: Mocha.Context): Promise<void> {
    const test = this.currentTest?.fullTitle() ?? "(unknown test)";
    let violations: unknown[] | null = null;
    try {
      // 読み込み直しの直後は main.ts がまだ動いていないことがあるので、少し待つ。
      await browser.waitUntil(
        async () => {
          // 読んだ分は取り除く(次のテストの結果と混ざらないようにする)。
          violations = await browser.execute(() => {
            const list = (window as unknown as { __cspViolations?: unknown[] }).__cspViolations;
            return list ? list.splice(0, list.length) : null;
          });
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
      problems.push(`${test}: ${String(err)}`);
      return;
    }
    const found = violations as unknown[] | null;
    if (found && found.length > 0) problems.push(`${test}: ${JSON.stringify(found)}`);
  },

  afterAll(): void {
    if (problems.length === 0) return;
    throw new Error(`CSP violations:\n${problems.splice(0, problems.length).join("\n")}`);
  },
};
