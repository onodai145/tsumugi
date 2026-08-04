export const COMPOSE_PLACEHOLDER_BANDS = {
  midnight: [
    "こんな時間に何してるの？",
    "寝なくても大丈夫？",
    "宇宙と交信する時間帯",
    "まだ起きてるんですか",
    "静かな夜ですね",
    "夜更かしは程々に",
    "こんな時間まで何を？",
  ],
  earlyMorning: [
    "早起きですね",
    "一日の始まり",
    "鳥より早い",
    "おはようございます（早い）",
    "静かな朝ですね",
    "今日は何をしますか？",
    "夜明け前ですね",
  ],
  morning: [
    "おはようございます",
    "今日も一日がんばりましょう",
    "モーニングルーティン",
    "朝ごはんは食べましたか？",
    "今日の予定は？",
    "気持ちのいい朝ですね",
    "通勤通学中ですか？",
  ],
  noon: [
    "いまどうしてる？",
    "お昼はもう食べた？",
    "早起きさんですね",
    "午後もがんばりましょう",
    "一息つきませんか？",
    "今日の調子はどう？",
    "お昼寝したい時間ですね",
  ],
  evening: [
    "お疲れさまです",
    "今日はどんな一日だった？",
    "夕焼けを見ながら",
    "帰り道ですか？",
    "一日お疲れさまでした",
    "夕食は何にしますか？",
    "空が綺麗な時間ですね",
  ],
  night: [
    "こんばんは",
    "今日もお疲れ様",
    "夜はこれから",
    "ゆっくりしていますか？",
    "明日の準備はできましたか？",
    "夜更けの時間",
    "おやすみ前のひととき",
  ],
} as const satisfies Record<string, readonly string[]>;

function bandForHour(hour: number): keyof typeof COMPOSE_PLACEHOLDER_BANDS {
  if (hour < 4) return "midnight";
  if (hour < 7) return "earlyMorning";
  if (hour < 10) return "morning";
  if (hour < 17) return "noon";
  if (hour < 19) return "evening";
  return "night";
}

export function pickComposePlaceholder(date: Date = new Date()): string {
  const phrases = COMPOSE_PLACEHOLDER_BANDS[bandForHour(date.getHours())];
  return phrases[Math.floor(Math.random() * phrases.length)];
}
