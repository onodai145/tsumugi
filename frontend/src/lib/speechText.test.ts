import { describe, expect, it } from "vitest";
import type { Note, User } from "../bindings/tauri.gen";
import {
  buildSpeechText,
  DEFAULT_COLUMN_SPEECH,
  MAX_AUTHOR_CHARS,
  MAX_SPEECH_CHARS,
  speechTextForColumnNote,
} from "./speechText";

function user(over: Partial<User> = {}): User {
  return {
    id: "u1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: null,
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
    ...over,
  } as User;
}

function note(over: Partial<Note> = {}): Note {
  return {
    id: "n1",
    createdAt: 0,
    text: "こんにちは",
    cw: null,
    visibility: "public",
    localOnly: false,
    user: user(),
    replyId: null,
    replyUserId: null,
    reply: null,
    renoteId: null,
    renote: null,
    files: [],
    poll: null,
    tags: [],
    mentions: [],
    emojis: {},
    channelId: null,
    via: null,
    ...over,
  } as Note;
}

const noAuthor = { speakAuthor: false };
const withAuthor = { speakAuthor: true };

describe("buildSpeechText: 何を読むか", () => {
  it("本文をそのまま読む", () => {
    expect(buildSpeechText(note({ text: "今日はいい天気" }), noAuthor)).toBe("今日はいい天気");
  });

  it("CW があれば CW だけを読み、本文は読まない", () => {
    const n = note({ cw: "ネタバレ注意", text: "犯人は〇〇" });
    expect(buildSpeechText(n, noAuthor)).toBe("ネタバレ注意");
  });

  it("純粋なリノート(本文なし)は読まない", () => {
    expect(buildSpeechText(note({ text: null, renoteId: "r1", renote: note({ id: "r1" }) }), noAuthor)).toBeNull();
  });

  it("引用リノートは、コメント部分だけを読む", () => {
    const n = note({ text: "これ好き", renoteId: "r1", renote: note({ id: "r1", text: "元のノート" }) });
    expect(buildSpeechText(n, noAuthor)).toBe("これ好き");
  });

  it("画像だけのノート(本文なし)は読まない", () => {
    expect(buildSpeechText(note({ text: null, files: [{ id: "f" } as never] }), noAuthor)).toBeNull();
  });

  it("空白・改行だけの本文は読まない", () => {
    expect(buildSpeechText(note({ text: "  \n 　 " }), noAuthor)).toBeNull();
    expect(buildSpeechText(note({ text: "" }), noAuthor)).toBeNull();
  });
});

describe("buildSpeechText: MFM の整形", () => {
  it("装飾(太字・small・fn)は中身をそのまま読む", () => {
    expect(buildSpeechText(note({ text: "**強調**と<small>小さい</small>と$[tada ほげ]" }), noAuthor)).toBe(
      "強調と小さいとほげ",
    );
  });

  it("メンションはユーザー名だけ(ホストは読まない)、ハッシュタグは # を除く", () => {
    expect(buildSpeechText(note({ text: "@bob@example.com こんにちは #rust" }), noAuthor)).toBe("bob こんにちは rust");
  });

  it("URL は「URL」と読み、ラベル付きリンクはラベルを読む", () => {
    expect(buildSpeechText(note({ text: "見て https://example.com/a?b=c です" }), noAuthor)).toBe("見て URL です");
    expect(buildSpeechText(note({ text: "[公式サイト](https://example.com)です" }), noAuthor)).toBe("公式サイトです");
  });

  it("コードは内容を読まず「コード」と読む", () => {
    expect(buildSpeechText(note({ text: "実行は`rm -rf /`で" }), noAuthor)).toBe("実行はコードで");
    expect(buildSpeechText(note({ text: "```\nfn main() {}\n```" }), noAuthor)).toBe("コード");
  });

  it("カスタム絵文字・Unicode 絵文字・数式は読まない", () => {
    expect(buildSpeechText(note({ text: "やった:tada:🎉\\(x^2\\)" }), noAuthor)).toBe("やった");
  });

  it("絵文字・URL だけでも、URL は読む。絵文字だけなら読まない", () => {
    expect(buildSpeechText(note({ text: "🎉🎉:blob:" }), noAuthor)).toBeNull();
    expect(buildSpeechText(note({ text: "https://example.com" }), noAuthor)).toBe("URL");
  });

  it("改行は読点にし、句読点の直後の改行は足さない。連続する改行も1つにする", () => {
    expect(buildSpeechText(note({ text: "一行目\n二行目" }), noAuthor)).toBe("一行目、二行目");
    expect(buildSpeechText(note({ text: "一行目。\n二行目" }), noAuthor)).toBe("一行目。二行目");
    expect(buildSpeechText(note({ text: "a\n\n\nb" }), noAuthor)).toBe("a、b");
  });

  it("先頭・末尾の読点や空白は落とす", () => {
    expect(buildSpeechText(note({ text: "\n\nこんにちは\n" }), noAuthor)).toBe("こんにちは");
  });
});

describe("buildSpeechText: 長さ", () => {
  it("上限ちょうどは切らない", () => {
    const text = "あ".repeat(MAX_SPEECH_CHARS);
    expect(buildSpeechText(note({ text }), noAuthor)).toBe(text);
  });

  it("上限を超えたら、切って「以下略」を付ける", () => {
    const out = buildSpeechText(note({ text: "あ".repeat(MAX_SPEECH_CHARS + 50) }), noAuthor)!;
    expect(out).toBe("あ".repeat(MAX_SPEECH_CHARS) + "、以下略");
  });

  it("サロゲートペアを途中で割らない", () => {
    const out = buildSpeechText(note({ text: "𠮷".repeat(MAX_SPEECH_CHARS + 5) }), noAuthor)!;
    expect(out).toBe("𠮷".repeat(MAX_SPEECH_CHARS) + "、以下略");
  });

  it("巨大な本文でも一瞬で終わる(数万文字)", () => {
    const started = performance.now();
    const out = buildSpeechText(note({ text: "あいう ".repeat(20_000) }), noAuthor)!;
    expect(performance.now() - started).toBeLessThan(1000);
    expect(out.endsWith("、以下略")).toBe(true);
  });

  it("長い空白の連続でも一瞬で終わる(正規表現が O(n²) にならない)", () => {
    const started = performance.now();
    const out = buildSpeechText(note({ text: " ".repeat(50_000) + "a" }), noAuthor);
    expect(performance.now() - started).toBeLessThan(100);
    expect(out).toBeNull(); // 先頭の上限までが空白だけ = 読む中身がない
  });

  it("上限で切った本文は、整形後が短くなっても「以下略」を付ける", () => {
    const out = buildSpeechText(note({ text: "a" + " ".repeat(50_000) + "b" }), noAuthor);
    expect(out).toBe("a、以下略");
  });

  it("表示名の長い空白の連続でも一瞬で終わる", () => {
    const started = performance.now();
    buildSpeechText(note({ text: "こんにちは", user: user({ name: " ".repeat(50_000) + "x" }) }), withAuthor);
    expect(performance.now() - started).toBeLessThan(100);
  });

  it("深くネストした MFM でも落ちない", () => {
    const text = "<small>".repeat(200) + "中身" + "</small>".repeat(200);
    expect(() => buildSpeechText(note({ text }), noAuthor)).not.toThrow();
  });
});

describe("buildSpeechText: 投稿者名", () => {
  it("speakAuthor がオンなら「{表示名}さん、」を前置きする", () => {
    expect(buildSpeechText(note({ text: "おはよう" }), withAuthor)).toBe("Aliceさん、おはよう");
  });

  it("表示名のカスタム絵文字は読まない。空になったら username を読む", () => {
    expect(buildSpeechText(note({ text: "おはよう", user: user({ name: "Alice:blob:" }) }), withAuthor)).toBe("Aliceさん、おはよう");
    expect(buildSpeechText(note({ text: "おはよう", user: user({ name: ":blob:" }) }), withAuthor)).toBe("aliceさん、おはよう");
    expect(buildSpeechText(note({ text: "おはよう", user: user({ name: null }) }), withAuthor)).toBe("aliceさん、おはよう");
  });

  it("長すぎる表示名は、上限(40文字)で切る(合計が Rust の上限を超えて黙って読まれなくならない)", () => {
    const out = buildSpeechText(note({ text: "おはよう", user: user({ name: "あ".repeat(300) }) }), withAuthor)!;
    expect(out).toBe("あ".repeat(MAX_AUTHOR_CHARS) + "さん、おはよう");
    // 本文が上限いっぱいでも、合計は Rust の受け付ける 300 文字に収まる。
    const long = buildSpeechText(note({ text: "い".repeat(500), user: user({ name: "あ".repeat(300) }) }), withAuthor)!;
    expect(Array.from(long).length).toBeLessThanOrEqual(300);
  });

  it("読む中身がなければ、投稿者名だけを読み上げない", () => {
    expect(buildSpeechText(note({ text: "🎉" }), withAuthor)).toBeNull();
    expect(buildSpeechText(note({ text: null }), withAuthor)).toBeNull();
  });

  it("CW のノートでも投稿者名を前置きする", () => {
    expect(buildSpeechText(note({ cw: "注意", text: "本文" }), withAuthor)).toBe("Aliceさん、注意");
  });
});

describe("speechTextForColumnNote: 発火の判定", () => {
  const col = { ...DEFAULT_COLUMN_SPEECH, enabled: true };

  it("全部の条件が揃ったときだけ文字列を返す", () => {
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: true, column: col })).toBe("Aliceさん、こんにちは");
  });

  it("自分のノート・全体オフ・カラムオフのどれでも null", () => {
    expect(speechTextForColumnNote(note(), { isOwn: true, globalEnabled: true, column: col })).toBeNull();
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: false, column: col })).toBeNull();
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: true, column: DEFAULT_COLUMN_SPEECH })).toBeNull();
  });

  it("カラムの speakAuthor に従う", () => {
    const c = { ...col, speakAuthor: false };
    expect(speechTextForColumnNote(note(), { isOwn: false, globalEnabled: true, column: c })).toBe("こんにちは");
  });

  it("既定のカラム設定は、オフ・投稿者名あり・上限5・声は継承", () => {
    expect(DEFAULT_COLUMN_SPEECH).toEqual({ enabled: false, speakAuthor: true, queueLimit: 5, voiceId: "" });
  });
});
