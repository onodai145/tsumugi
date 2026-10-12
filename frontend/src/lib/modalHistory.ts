// モバイルの全画面モーダル用の履歴管理(Issue #450)。
//
// 全画面表示中のモーダルごとに history.pushState で履歴を1つ積み、戻るジェスチャ/ボタン
// (Android WebViewではgoBack→popstate)で最上段のモーダルだけを閉じる。
// UI側(←ボタン・Escape・保存後の自動クローズ)で閉じたときは、積んだ履歴を戻し、
// その popstate は無視カウンタで握りつぶして onclose を二重に呼ばない。
//
// 同一tickに複数枚閉じた場合は history.back() を連続で呼ばず、マイクロタスクで束ねて
// history.go(-k) を1回だけ発行する。連続 back() が何回 popstate を起こすかは実装依存
// (jsdomは1回に畳む)で、無視カウンタとずれると onclose の二重呼び出しや履歴の取りこぼしに
// なるため、popstate が必ず1回になる go(-k) に統一している。

type Entry = { id: number; onclose: () => void; popped: boolean };

const stack: Entry[] = [];
// 履歴を戻す操作の完了待ちの間に積もうとしたエントリ。履歴の移動は非同期なので、完了前に
// pushState すると後から来る移動が新しい履歴を消してしまう。
const deferred: Entry[] = [];
let nextId = 0;
// UI側で閉じたことで発行した history.go() の popstate をあと何回無視するか
let ignoredPops = 0;
// まだ history.go() を発行していない、戻すべき履歴の数(同一tick内でまとめる)
let pendingBack = 0;
let backScheduled = false;

function scheduleBack() {
  pendingBack++;
  if (backScheduled) return;
  backScheduled = true;
  queueMicrotask(() => {
    backScheduled = false;
    const k = pendingBack;
    pendingBack = 0;
    if (k === 0) return;
    ignoredPops++;
    history.go(-k);
  });
}

function doPush(entry: Entry) {
  history.pushState({ tsumugiModal: entry.id }, "");
}

function flushDeferred() {
  for (const entry of deferred.splice(0)) doPush(entry);
}

function onPopState() {
  if (ignoredPops > 0) {
    ignoredPops--;
    if (ignoredPops === 0) flushDeferred();
    return;
  }
  const top = stack[stack.length - 1];
  if (!top) return;
  // 戻る操作で履歴は既に1つ戻っているため、アンマウント時に back() しない
  top.popped = true;
  top.onclose();
}

if (typeof window !== "undefined") {
  window.addEventListener("popstate", onPopState);
}

/// 全画面モーダルの表示開始時に呼ぶ。戻り値のreleaseは、モーダルが閉じられたとき(アンマウント時)に呼ぶ。
export function pushModalHistory(onclose: () => void): () => void {
  const entry: Entry = { id: ++nextId, onclose, popped: false };
  stack.push(entry);
  if (ignoredPops > 0 || pendingBack > 0) deferred.push(entry);
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
