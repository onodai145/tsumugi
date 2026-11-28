// CSP 違反の記録(Issue #612)。console.warn に出し、__cspViolations に積む。E2E は各テストの後に
// この配列が空であることを確かめる。WebView が読み込み直されると、それまでの記録は消える。

/// 遮断された画像が並ぶ画面などで増え続けないようにする上限。
export const CSP_VIOLATIONS_LIMIT = 50;

export interface CspViolation {
  directive: string;
  blockedUri: string;
  source: string;
  line: number;
}

type CspTarget = EventTarget & { __cspViolations?: CspViolation[] };

declare global {
  interface Window {
    __cspViolations?: CspViolation[];
  }
}

export function installCspViolationRecorder(target: CspTarget = window): void {
  const list = (target.__cspViolations ??= []);
  let dropping = false;
  target.addEventListener("securitypolicyviolation", (event) => {
    if (list.length >= CSP_VIOLATIONS_LIMIT) {
      // 黙って捨てると調べにくいので、捨て始めたときに 1 回だけ知らせる。
      if (!dropping) console.warn(`[csp] ${CSP_VIOLATIONS_LIMIT} violations recorded; further ones are dropped`);
      dropping = true;
      return;
    }
    dropping = false;
    const e = event as SecurityPolicyViolationEvent;
    const v = { directive: e.effectiveDirective, blockedUri: e.blockedURI, source: e.sourceFile, line: e.lineNumber };
    list.push(v);
    console.warn(`[csp] blocked ${v.directive} ${v.blockedUri} (${v.source}:${v.line})`);
  });
}
