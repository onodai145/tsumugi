// 要素を document.body 直下へ移動するSvelteアクション。
// 固定配置のポップアップ/メニューを、祖先要素の overflow:hidden や
// position:relative の影響を受けずに画面へ重ねて表示するために使う。
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy: () => node.remove(),
  };
}
