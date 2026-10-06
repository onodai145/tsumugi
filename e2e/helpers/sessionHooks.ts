// 再起動をまたぐシナリオ用: セッションの合間(アプリ停止中)にMisskey側/設定ファイルを操作する下準備(wdioの
// beforeSession フックから呼ぶ)と、spec間で受け渡す状態ファイル。
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { muteUser, unmuteUser } from "./misskeyApi";

const __dirname = dirname(fileURLToPath(import.meta.url));
const STATE_FILE = join(__dirname, "..", "wdio-logs", "restart-state.json");

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type State = Record<string, any>;

export function loadState(): State {
  return existsSync(STATE_FILE) ? JSON.parse(readFileSync(STATE_FILE, "utf-8")) : {};
}
export function saveState(patch: State): void {
  writeFileSync(STATE_FILE, JSON.stringify({ ...loadState(), ...patch }, null, 2));
}

function settingsPath(): string {
  const home = readFileSync(process.env.E2E_REUSE_HOME_FILE as string, "utf-8").trim();
  return join(home, "config", "com.onodai.tsumugi", "settings.json");
}

const log = (m: string) => console.log(`[e2e:pre] ${m}`);

export async function runPreSession(specs: string[]): Promise<void> {
  const dir = basename(dirname(specs[0] ?? ""));
  const name = basename(specs[0] ?? "");
  const st = loadState();
  // 直前のセッションのアプリが完全に止まるのを待つ(run-app.shのwatchdogが後始末するため)
  if (dir === "specs-server-mute-restart" && /^[23]-/.test(name)) await new Promise((r) => setTimeout(r, 4000));

  if (dir === "specs-server-mute-restart" && name.startsWith("2-")) {
    // ミュートを**追加**する(解除ではない)。さらに、acc2の前回スナップショットを消して「起動時の初回同期」を再現する。
    await muteUser(st.c.adminToken, st.c.xId);
    log(`admin muted extra user X (${st.c.xId})`);
    const p = settingsPath();
    const json = JSON.parse(readFileSync(p, "utf-8"));
    delete json.server_mute_snapshots?.[st.c.acc2Id];
    writeFileSync(p, JSON.stringify(json));
    log(`removed server_mute_snapshots[acc2=${st.c.acc2Id}] from settings.json`);
  }
  if (dir === "specs-server-mute-restart" && name.startsWith("3-")) {
    await unmuteUser(st.c.adminToken, st.c.mId);
    log(`admin UNMUTED user M (${st.c.mId})`);
  }
}
