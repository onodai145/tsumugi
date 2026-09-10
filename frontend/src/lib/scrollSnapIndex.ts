// Scroll Snapコンテナの現在のscrollLeftから、実際に着地したスロットのインデックスを
// 解決する純粋関数(Issue #296)。タブ用・カラム用のscrollend(または相当のデバウンス)
// ハンドラの両方から使う。

/// scrollLeftをcontainerWidthで割って最も近いスロットインデックスに丸め、
/// 0..slotCount-1の範囲にクランプする。オーバースクロール(負の値/範囲外)にも耐える。
export function resolveSettledIndex(scrollLeft: number, containerWidth: number, slotCount: number): number {
  if (containerWidth <= 0 || slotCount <= 0) return 0;
  const raw = Math.round(scrollLeft / containerWidth);
  return Math.max(0, Math.min(slotCount - 1, raw));
}
