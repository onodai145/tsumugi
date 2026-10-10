// クライアント側予約(Issue #60 B)の E2E 用 wdio 設定。デバッグビルド限定の環境変数
// TSUMUGI_DEBUG_SERVER_VERSION は package.json のスクリプトで設定する(この設定を直接使う場合も、
// 同じ環境変数を付けること)。
import type { Options } from "@wdio/types";
import { config as base } from "./wdio.conf";

export const config: Options.Testrunner = {
  ...base,
  specs: ["./specs-scheduled/**/*.e2e.ts"],
};
