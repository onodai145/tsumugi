import { extract, parse } from "mfm-js";
import type { MfmNode } from "mfm-js";

/// 本文中のMFM `url`ノード（裸URL）のURLを重複排除して返す。
/// カスタムテキストの`link`ノード（`[text](url)`）は対象外（Issue #9）。
export function extractPreviewUrls(text: string): string[] {
  if (!text) return [];
  const nodes = extract(parse(text), (node) => node.type === "url") as Extract<
    MfmNode,
    { type: "url" }
  >[];
  return [...new Set(nodes.map((n) => n.props.url))];
}
