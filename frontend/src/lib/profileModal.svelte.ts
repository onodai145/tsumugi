export type ProfileTarget = { userId: string } | { username: string; host: string | null };

let target = $state<ProfileTarget | null>(null);
let accountId = $state<string | null>(null);

/// プロフィールモーダルを開く。`accountId` を省略した場合、呼び出し側（ProfileModal）が
/// app.defaultAccountId() にフォールバックする（mentionクリック等、経路上にaccountIdが無い場合用）。
export function openProfile(t: ProfileTarget, forAccountId?: string): void {
  target = t;
  accountId = forAccountId ?? null;
}

export function closeProfile(): void {
  target = null;
  accountId = null;
}

export function currentProfileTarget(): ProfileTarget | null {
  return target;
}

export function currentProfileAccountId(): string | null {
  return accountId;
}
