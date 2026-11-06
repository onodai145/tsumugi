/// 再認証画面の説明文(AddAccount.svelte の再認証フロー)。
/// needsLogin が true なら、設定のインポートで追加されたトークンなしのアカウント。
/// false なら、トークンはあるがスコープ更新などで取り直すアカウント。
export function reauthMessage(needsLogin: boolean): string {
  return needsLogin
    ? "このアカウントは設定のインポートで追加され、まだログインしていません。ブラウザで認可を完了したら、下のボタンを押してください。"
    : "スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。";
}
