import { config as base } from "./wdio.conf";

// モバイルUI検証専用(Issue #259)。デスクトップspecsとはウィンドウサイズ前提が
// 異なるため、specs-mobile/ を別configで実行する。
export const config = {
  ...base,
  specs: ["./specs-mobile/**/*.e2e.ts"],
};
