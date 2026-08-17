import type { Options } from "@wdio/types";

export const config: Options.Testrunner = {
  runner: "local",
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  services: [["tauri", { driverProvider: "external", autoInstallTauriDriver: true }]],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: "../src-tauri/target/debug/tsumugi",
      },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "info",
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },
};
