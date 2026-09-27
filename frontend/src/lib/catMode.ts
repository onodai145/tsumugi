import { app } from "./store.svelte";

/**
 * 設定(catMode)を加味して「猫として扱うか」を返す（Issue #42）。
 * "cat" = 常に猫 / "human" = 常に人間 / それ以外("respect"・未設定) = ユーザーの isCat に従う。
 */
export function effectiveIsCat(isCat: boolean): boolean {
  switch (app.ui.catMode) {
    case "cat":
      return true;
    case "human":
      return false;
    default:
      return isCat;
  }
}
