// メンション/ハッシュタグ補完のIPC呼び出し + CompletionItem変換。
// mfmCompletion.ts は DOM非依存の純粋関数のみという責務を保つため、
// 副作用(IPC呼び出し)を持つこのロジックは別ファイルに分離する。
import { commands, unwrap } from "./ipc";
import type { CompletionItem } from "./mfmCompletion";

export async function searchMentionItems(accountId: string, query: string): Promise<CompletionItem[]> {
  const users = await unwrap(commands.searchUsers(accountId, query));
  return users.map((u) => {
    const acct = u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
    return {
      key: `user:${u.id}`,
      label: acct,
      insertText: acct,
      thumbnail: u.avatarUrl ? { type: "avatar" as const, url: u.avatarUrl } : undefined,
    };
  });
}

export async function searchHashtagItems(accountId: string, query: string): Promise<CompletionItem[]> {
  const tags = await unwrap(commands.searchHashtags(accountId, query));
  return tags.map((tag) => ({ key: `tag:${tag}`, label: `#${tag}`, insertText: `#${tag}` }));
}
