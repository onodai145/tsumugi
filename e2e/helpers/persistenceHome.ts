// settings-persistence-restart.part1/2.e2e.ts が共有する、run-app.shへ渡す
// E2E_REUSE_HOME_FILE の固定パス。part1が新規作成したTMP_HOMEのパスをここに書き込み、
// part2が同じパスを読んで同じTMP_HOMEを再利用することでアプリ再起動を再現する。
//
// この値は"e2e:persistence"スクリプト(package.json)が設定する
// E2E_REUSE_HOME_FILE=./wdio-logs/persistence-home-path.txt と一致していなければ
// ならない。part1/part2は個別のwdioセッション(=run-app.shの個別起動)だが、
// どちらも同じ"wdio run"プロセス(=同一のprocess.env)から--specで連続起動される
// ため、この環境変数がrun-app.shまで伝播する
// (node_modules/@wdio/tauri-service確認済み: アプリプロセスは
// `env: {...process.env, ...options.env}`で起動される)。
import { join } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
export const PERSISTENCE_HOME_FILE = join(__dirname, "..", "wdio-logs", "persistence-home-path.txt");
