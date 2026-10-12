// モバイルの全画面モーダル用の履歴管理(Issue #450)。
//
// 全画面表示中のモーダルごとに history.pushState で履歴を1つ積み、戻るジェスチャ/ボタン
// (Android WebViewではgoBack→popstate)で閉じる。どのモーダルを閉じるかは、popstate が
// 着地した履歴の state(積んだときのid)で決める: 着地先より後に積んだモーダルは履歴から
// 外れているので閉じる。件数のカウントには頼らないため、WebViewが履歴を飛ばしても、
// popstate が遅れて届いても、取りこぼしや二重クローズにならない。
// UI側(←ボタン・Escape・保存後の自動クローズ)で閉じたときは、積んだ履歴を戻す。
//
// 同一tickに複数枚閉じた場合は history.back() を連続で呼ばず、マイクロタスクで束ねて
// history.go(-k) を1回だけ発行する(連続 back() が何回 popstate を起こすかは実装依存で、
// jsdomは1回に畳む)。

type Entry = { id: number; onclose: () => void; pushed: boolean; popped: boolean };

// popstate が来ないまま履歴の移動待ちで固まらないための猶予(ms)
const GO_TIMEOUT_MS = 1000;

const stack: Entry[] = [];
// 履歴を戻す操作の完了待ちの間に積もうとしたエントリ。履歴の移動は非同期なので、完了前に
// pushState すると後から来る移動が新しい履歴を消してしまう。
const deferred: Entry[] = [];
// 発行済みで popstate 待ちの history.go() の数(遅延pushを再開するタイミングの判定にだけ使う)
let inFlight = 0;
// まだ history.go() を発行していない、戻すべき履歴の数(同一tick内でまとめる)
let pendingBack = 0;
let backScheduled = false;

// idは「後から積んだほど大きい」ことだけが必要。モジュールが複数回評価されても(HMR・テスト)
// 衝突しないよう、グローバルの連番を使う。
const seq = globalThis as { __tsumugiModalSeq?: number };
function nextId(): number {
  seq.__tsumugiModalSeq = (seq.__tsumugiModalSeq ?? 0) + 1;
  return seq.__tsumugiModalSeq;
}

function doPush(entry: Entry) {
  history.pushState({ tsumugiModal: entry.id }, "");
  entry.pushed = true;
}

function flushDeferred() {
  for (const entry of deferred.splice(0)) doPush(entry);
}

function scheduleBack() {
  pendingBack++;
  if (backScheduled) return;
  backScheduled = true;
  queueMicrotask(() => {
    backScheduled = false;
    const k = pendingBack;
    pendingBack = 0;
    if (k === 0) return;
    inFlight++;
    history.go(-k);
    // popstate が来なかった場合でも、遅延していたpushを再開できるようにする
    setTimeout(() => {
      if (inFlight > 0) {
        inFlight = 0;
        flushDeferred();
      }
    }, GO_TIMEOUT_MS);
  });
}

function onPopState(e: PopStateEvent) {
  const landed = (e.state as { tsumugiModal?: number } | null)?.tsumugiModal ?? 0;
  // 着地先より後に積んだモーダルは履歴から外れている。上から順に閉じる。
  for (const entry of [...stack].reverse()) {
    if (entry.pushed && !entry.popped && entry.id > landed) {
      // 履歴は既に戻っているため、アンマウント時に go() しない
      entry.popped = true;
      entry.onclose();
    }
  }
  // 閉じる判定の後に遅延分を積む(積んだ直後のエントリを誤って閉じないため)
  if (inFlight > 0 && --inFlight === 0) flushDeferred();
}

if (typeof window !== "undefined") {
  window.addEventListener("popstate", onPopState);
}

/// 全画面モーダルの表示開始時に呼ぶ。戻り値のreleaseは、モーダルが閉じられたとき(アンマウント時)に呼ぶ。
export function pushModalHistory(onclose: () => void): () => void {
  const entry: Entry = { id: nextId(), onclose, pushed: false, popped: false };
  stack.push(entry);
  if (inFlight > 0 || pendingBack > 0) deferred.push(entry);
  else doPush(entry);

  return () => {
    const i = stack.indexOf(entry);
    if (i < 0) return;
    stack.splice(i, 1);
    const d = deferred.indexOf(entry);
    if (d >= 0) {
      // まだ履歴を積んでいないので戻す必要も無い
      deferred.splice(d, 1);
      return;
    }
    if (entry.popped) return;
    scheduleBack();
  };
}
