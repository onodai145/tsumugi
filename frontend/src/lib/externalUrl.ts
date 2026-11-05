// 外部サービスのURL(検索エンジン、URLプレビューの取得先)の検証(Issue #549)。
// Rust側(src-tauri/src/domain/ui.rs)が正で、ここは保存前に即座にエラーを出すための同じ規則。

export const MAX_EXTERNAL_URL_LEN = 2048;
const QUERY_PLACEHOLDER = "{query}";

/// `http` / `https` で、ホストがあり、認証情報を含まないURLだけ通す。通らなければメッセージを返す。
function parseHttpUrl(raw: string): URL | string {
  if (raw.length > MAX_EXTERNAL_URL_LEN) return `URLが長すぎます(${MAX_EXTERNAL_URL_LEN}文字まで)`;
  // `URL` は前後の空白や途中のタブ・改行を黙って取り除くので、検証した形と実際に使う文字列が食い違う。
  // 前後の空白と制御文字を含む値は拒否する。
  if (raw !== raw.trim() || /[\u0000-\u001f\u007f-\u009f]/.test(raw)) {
    return "URLの前後の空白や制御文字(改行・タブなど)は含められません";
  }
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    return "URLの形式が正しくありません";
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return "URLのスキームはhttpまたはhttpsにしてください";
  }
  if (!parsed.hostname) return "URLにホストがありません";
  if (parsed.username || parsed.password) return "URLにユーザー名やパスワードは含められません";
  return parsed;
}

/// 検索エンジンURLのテンプレートを検証する。問題があればメッセージ、なければ `null`。
/// `{query}` を `x` と `y` に置換した2つのURLのスキーム・ホスト(ポート込み)が一致することで、
/// `{query}` が宛先の位置に入っていないことを確かめる。
export function validateSearchEngineUrl(template: string): string | null {
  if (!template.includes(QUERY_PLACEHOLDER)) return "検索URLには{query}を含めてください";
  const withX = parseHttpUrl(template.replace(QUERY_PLACEHOLDER, "x"));
  if (typeof withX === "string") return withX;
  const withY = parseHttpUrl(template.replace(QUERY_PLACEHOLDER, "y"));
  if (typeof withY === "string") return withY;
  if (withX.protocol !== withY.protocol || withX.host !== withY.host) {
    return "{query}はホストやポートの位置には置けません";
  }
  return null;
}

/// URLプレビューの取得先を検証する。空欄(空白のみを含む)は「接続先インスタンスの /url」で有効。
export function validateSummalyProxyUrl(raw: string): string | null {
  if (raw.trim() === "") return null;
  const parsed = parseHttpUrl(raw);
  return typeof parsed === "string" ? parsed : null;
}
