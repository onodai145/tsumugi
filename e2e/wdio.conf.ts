import type { Options } from "@wdio/types";

export const config: Options.Testrunner = {
  runner: "local",
  // settings-persistence-restart.part1/2.e2e.ts は専用の "pnpm e2e:persistence"
  // (E2E_REUSE_HOME_FILEを設定した上でpart1→part2をこの順で--spec指定する)経由でのみ
  // 正しく動作する(E2E_REUSE_HOME_FILEを設定せずに実行すると、part2は「アカウント
  // 追加画面が出ないこと」のアサーションで必ず失敗する)。WebdriverIOの`specs`配列は
  // `!`によるnegationパターンをサポートしていない(`glob.sync('!...')`は常に空配列を
  // 返す)ため、パターンによる除外はできない。代わりにこの2ファイルを`./specs/`とは
  // 別ディレクトリ(`./specs-persistence/`)に置くことで、この`./specs/**/*.e2e.ts`
  // 一本のパターンから自動的に除外している(Issue #223)。
  specs: ["./specs/**/*.e2e.ts"],
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
