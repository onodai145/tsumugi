// 再起動をまたぐシナリオ・DB検査を伴うシナリオ用のwdio設定(Issue #458)。再起動をまたぐシナリオで、セッション(=アプリの起動)の合間に
// Misskey側の状態を変えるため、wdioの beforeSession フックでspecごとの下準備を走らせる。
import type { Options } from "@wdio/types";
import { config as base } from "./wdio.conf";
import { runPreSession } from "./helpers/sessionHooks";

export const config: Options.Testrunner = {
  ...base,
  beforeSession: async (_c, _caps, specs) => {
    await runPreSession(specs ?? []);
  },
};
