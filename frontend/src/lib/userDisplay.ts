export function displayName(u: { name: string | null; username: string }): string {
  return u.name ?? u.username;
}

export function acct(u: { username: string; host: string | null }): string {
  return u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
}
