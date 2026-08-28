// 任意インスタンスの themeColor（Instance Ticker用、Issue #103）は可読性が保証されない
// 第三者由来の色なので、相対輝度から自動で黒/白の文字色を選ぶ。
// 参考: WCAG 2.0 の相対輝度式 (https://www.w3.org/TR/WCAG20/#relativeluminancedef)
const HEX_RE = /^#([0-9a-fA-F]{6})$/;

// Instance Ticker の themeColor はリモートインスタンス管理者が任意の文字列を設定できる
// ため、style属性へ直接埋め込む前に「hexカラーリテラルそのものである」ことを検証する。
// `;` などを含む値を通すとCSSインジェクション（UIリドレス）につながるため、
// #rgb/#rgba/#rrggbb/#rrggbbaa のいずれか以外は一切許可しない。
const VALID_HEX_COLOR_RE = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;

export function isValidHexColor(value: string): boolean {
  return VALID_HEX_COLOR_RE.test(value);
}

function srgbToLinear(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function readableTextColor(hex: string): "#000000" | "#ffffff" {
  const m = HEX_RE.exec(hex);
  if (!m) return "#ffffff";
  const n = parseInt(m[1], 16);
  const r = srgbToLinear((n >> 16) & 0xff);
  const g = srgbToLinear((n >> 8) & 0xff);
  const b = srgbToLinear(n & 0xff);
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  // 背景の相対輝度が高い(明るい)ほど黒文字が読みやすい。しきい値0.179は
  // WCAG的に「白文字とのコントラスト比 >= 4.5」が概ね崩れ始める境目。
  return luminance > 0.179 ? "#000000" : "#ffffff";
}
