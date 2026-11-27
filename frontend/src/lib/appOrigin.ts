// アプリ本体を、外部由来の URL から iframe に読み込ませないための判定(Issue #613)。
// Windows / Android ではアプリが http://tauri.localhost で配信されるので、http / https の確認だけでは通ってしまう。

/// 外部由来の URL を、リンクや iframe の src にしてよいか判定する。
/// http / https の絶対 URL のうち、アプリ自身のオリジンと localhost 名を除く。
/// `*.localhost` は予約名(RFC 6761)で、Tauri がカスタムプロトコル(tauri / asset / ipc)の割り当てに使う。
export function isSafeUrl(url: string, ownOrigin: string = location.origin): boolean {
  if (!/^https?:\/\//i.test(url)) return false;
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  const host = parsed.hostname.replace(/\.$/, "");
  return parsed.origin !== ownOrigin && host !== "localhost" && !host.endsWith(".localhost");
}

/// 別の文書の中(iframe 等)で読み込まれたか。URL の判定は、リダイレクトや iframe 内の自己遷移を
/// 止められないので、起動する側でも確認する。
export function isFramed(win: Window = window): boolean {
  return win.self !== win.top;
}
