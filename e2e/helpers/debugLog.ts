// CIの`account → post → reaction`失敗調査用の診断ロガー。
//
// wdio.conf.tsの`outputDir: "./wdio-logs"`と、
// .github/workflows/test.ymlの`Upload E2E artifacts on failure`ステップが
// 拾う`e2e/wdio-logs/`にファイルを書く。Playwright側(miauthBridge.ts /
// account-post-reaction.e2e.tsのclickThroughAccountSelect())の処理は
// WebDriverのセッションを経由しないため、wdio.logには一切現れない
// (2026-08-19実行のCI失敗調査で確認済み: elementClick("add-account-start")の
// RESULTからTauriプラグイン待受の再開まで16.5秒、wdio.logに何も出ない)。
// console.log()もwdio workerの標準出力転送に依存せず確実に残すため、
// ここでファイルへも直接追記する。
import { appendFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const LOG_DIR = join(__dirname, "..", "wdio-logs");
const LOG_FILE = join(LOG_DIR, "miauth-bridge-debug.log");

let dirEnsured = false;
function ensureDir(): void {
  if (dirEnsured) return;
  mkdirSync(LOG_DIR, { recursive: true });
  dirEnsured = true;
}

/**
 * console.log（wdio worker経由でCIジョブログに出るが転送はベストエフォート）
 * と、e2e/wdio-logs/miauth-bridge-debug.log（CI失敗時に確実にアーティファクト
 * として回収される）の両方へ、タイムスタンプ付きで同じ行を書く。
 */
export function debugLog(tag: string, message: string): void {
  const line = `[${new Date().toISOString()}] [${tag}] ${message}`;
  // eslint-disable-next-line no-console
  console.log(line);
  try {
    ensureDir();
    appendFileSync(LOG_FILE, line + "\n");
  } catch (err) {
    // eslint-disable-next-line no-console
    console.log(`[${new Date().toISOString()}] [debugLog] failed to write ${LOG_FILE}: ${String(err)}`);
  }
}

export function debugLogPath(name: string): string {
  ensureDir();
  return join(LOG_DIR, name);
}
