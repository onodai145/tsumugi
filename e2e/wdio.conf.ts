import type { Options } from "@wdio/types";

export const config: Options.Testrunner = {
  runner: "local",
  // settings-persistence-restart.part1/2.e2e.ts は専用の "pnpm e2e:persistence"
  // (E2E_REUSE_HOME_FILEを設定した上でpart1→part2をこの順で--spec指定する)経由でのみ
  // 正しく動作するため、既定のspec全実行からは除外する(Issue #223)。
  // E2E_REUSE_HOME_FILEを設定せずに実行すると、part2は「アカウント追加画面が出ない
  // こと」のアサーションで必ず失敗する。
  specs: ["./specs/**/*.e2e.ts", "!./specs/settings-persistence-restart.*.e2e.ts"],
  maxInstances: 1,
  services: [["tauri", { driverProvider: "external", autoInstallTauriDriver: true }]],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: "./scripts/run-app.sh",
      },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "info",
  outputDir: "./wdio-logs",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },
};
