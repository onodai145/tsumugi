// fixed 配置のポップオーバー(メニュー・補完候補)を、アンカーの下/上のどちらに出すかを決める。
//
// モバイルの投稿画面ではボタン群が画面下端(ソフトキーボードの直上)に並ぶため、メニューを
// 下向きに出すと画面外やキーボードの裏に隠れる。WebView はキーボードで縮まないので
// window.innerHeight だけでは足りず、ネイティブが設定する --keyboard-inset を引いた値を
// 「見える領域の下端」として使う(Issue: モバイル投稿画面のキーボード対応)。

export type VerticalPlacement = { top: number; bottom?: undefined } | { bottom: number; top?: undefined };

/// ソフトキーボードが WebView 下端を覆っている高さ(CSS px)。ネイティブが --keyboard-inset に設定する。
export function keyboardInset(): number {
  return parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--keyboard-inset")) || 0;
}

/// キーボードに隠れない領域の下端(viewport 座標)。
export function visibleBottom(): number {
  return window.innerHeight - keyboardInset();
}

/// アンカーの下に出すか上に出すかを決める。下に収まる、または下の方が広ければ下。
/// 上に出すときは `bottom`(viewport 下端からの距離)で指定するため、メニューの高さが
/// 見積もりと違っても、アンカーの直上に貼り付く。
export function placeVertically(
  anchor: { top: number; bottom: number },
  menuHeight: number,
  gap = 4,
): VerticalPlacement {
  const spaceBelow = visibleBottom() - anchor.bottom;
  const spaceAbove = anchor.top;
  if (spaceBelow >= menuHeight + gap * 2 || spaceBelow >= spaceAbove) return { top: anchor.bottom + gap };
  return { bottom: window.innerHeight - anchor.top + gap };
}

/// `position: fixed` 用の style 文字列(left と、top または bottom)。
export function placementStyle(left: number, p: VerticalPlacement): string {
  return `left:${left}px;` + (p.top !== undefined ? `top:${p.top}px` : `bottom:${p.bottom}px`);
}
