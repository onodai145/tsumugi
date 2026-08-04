# 投稿欄プレースホルダー時間帯変更 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 投稿欄 (`ComposeBar.svelte`) のプレースホルダーを、現在時刻の時間帯に応じて複数パターンからランダム表示するようにする。

**Architecture:** 時間帯判定とランダム選択のロジックを純粋関数として `frontend/src/lib/composePlaceholder.ts` に切り出し、`ComposeBar.svelte` からは `text` が空になったタイミングでその関数を呼び出すだけにする。ロジックをコンポーネントから分離することで、Svelteのレンダリングやモックなしにvitestで境界値をテストできる。

**Tech Stack:** TypeScript, Svelte 5 (runes: `$state`, `$effect`), vitest (`vi.useFakeTimers` / `vi.setSystemTime`)

## Global Constraints

- 時間帯区分は6区分、境界は「開始時刻を含み終了時刻を含まない」: 深夜0–4時、早朝4–7時、朝7–10時、昼10–17時、夕方17–19時、夜19–24時。
- 各区分の文言候補は7パターン。括弧書きの操作説明（Ctrl+Enter で投稿）は付けない。
- 再抽選は投稿欄のテキストが空になったタイミング（マウント時含む）のみ。入力中の再抽選はしない。
- 設定によるON/OFFトグルは追加しない。
- 文言候補・区分定義は `docs/superpowers/specs/2026-08-04-time-based-compose-placeholder-design.md` の一覧を正としてそのまま使う。

---

### Task 1: `composePlaceholder.ts` の実装とテスト

**Files:**
- Create: `frontend/src/lib/composePlaceholder.ts`
- Test: `frontend/src/lib/composePlaceholder.test.ts`

**Interfaces:**
- Produces: `pickComposePlaceholder(date?: Date): string` — 引数省略時は `new Date()` を使う。返り値は該当区分の候補配列のいずれか。

- [ ] **Step 1: Write the failing test**

`frontend/src/lib/composePlaceholder.test.ts` を作成する:

```typescript
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pickComposePlaceholder, COMPOSE_PLACEHOLDER_BANDS } from "./composePlaceholder";

function setHour(hour: number, minute = 0) {
  const d = new Date("2026-07-29T00:00:00");
  d.setHours(hour, minute, 0, 0);
  vi.setSystemTime(d);
}

describe("pickComposePlaceholder", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const cases: { hour: number; band: keyof typeof COMPOSE_PLACEHOLDER_BANDS }[] = [
    { hour: 0, band: "midnight" },
    { hour: 3, band: "midnight" },
    { hour: 4, band: "earlyMorning" },
    { hour: 6, band: "earlyMorning" },
    { hour: 7, band: "morning" },
    { hour: 9, band: "morning" },
    { hour: 10, band: "noon" },
    { hour: 16, band: "noon" },
    { hour: 17, band: "evening" },
    { hour: 18, band: "evening" },
    { hour: 19, band: "night" },
    { hour: 23, band: "night" },
  ];

  for (const { hour, band } of cases) {
    it(`returns a phrase from the "${band}" band at hour ${hour}`, () => {
      setHour(hour);
      const result = pickComposePlaceholder();
      expect(COMPOSE_PLACEHOLDER_BANDS[band]).toContain(result);
    });
  }

  it("uses the passed-in date instead of the system clock", () => {
    setHour(2); // 深夜のはずだが、引数の時刻(朝8時)を優先すべき
    const morning = new Date("2026-07-29T08:00:00");
    const result = pickComposePlaceholder(morning);
    expect(COMPOSE_PLACEHOLDER_BANDS.morning).toContain(result);
  });

  it("only ever returns phrases defined in one of the bands", () => {
    const allPhrases = new Set(Object.values(COMPOSE_PLACEHOLDER_BANDS).flat());
    for (let hour = 0; hour < 24; hour++) {
      setHour(hour);
      expect(allPhrases.has(pickComposePlaceholder())).toBe(true);
    }
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/lib/composePlaceholder.test.ts`
Expected: FAIL — `composePlaceholder.ts` に該当するモジュールが存在せずimportエラーになる。

- [ ] **Step 3: Write minimal implementation**

`frontend/src/lib/composePlaceholder.ts` を作成する:

```typescript
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/lib/composePlaceholder.test.ts`
Expected: PASS（全ケース）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/composePlaceholder.ts frontend/src/lib/composePlaceholder.test.ts
git commit -m "feat: 時間帯別プレースホルダーのロジックを追加"
```

---

### Task 2: `ComposeBar.svelte` への組み込み

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte:31` (`let text = $state("");` の直後に `placeholder` state を追加)
- Modify: `frontend/src/ui/ComposeBar.svelte:445` (`placeholder="いまどうしてる？（Ctrl+Enter で投稿）"` を差し替え)

**Interfaces:**
- Consumes: `pickComposePlaceholder(date?: Date): string` from `frontend/src/lib/composePlaceholder.ts` (Task 1)

- [ ] **Step 1: import文を追加**

`frontend/src/ui/ComposeBar.svelte` の先頭付近、既存の `import` 群（17行目付近、`bindings/tauri.gen` の import の前後）に以下を追加する:

```typescript
import { pickComposePlaceholder } from "../lib/composePlaceholder";
```

- [ ] **Step 2: placeholder state と再抽選effectを追加**

`let text = $state("");`(31行目) の直後に追加:

```typescript
  let placeholder = $state(pickComposePlaceholder());
  $effect(() => {
    if (text === "") {
      placeholder = pickComposePlaceholder();
    }
  });
```

- [ ] **Step 3: textareaのplaceholder属性を差し替え**

445行目の

```svelte
    placeholder="いまどうしてる？（Ctrl+Enter で投稿）"
```

を

```svelte
    placeholder={placeholder}
```

に変更する。

- [ ] **Step 4: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: エラーなし（既存のエラーが元々ある場合はそれらのみ残り、新規エラーが発生しないことを確認）

- [ ] **Step 5: 動作確認（devサーバー）**

Run: `cargo tauri dev` をプロジェクトルートで実行し、投稿欄のプレースホルダーが表示されることを目視確認する。テキストを入力→全消去したときにプレースホルダー文言が変わりうることを確認する（ランダムなので毎回変わるとは限らない）。確認後、devサーバーを終了する。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: 投稿欄プレースホルダーを時間帯に応じて表示"
```

---

## Self-Review Notes

- Spec coverage: 時間帯区分・文言候補・再抽選タイミング・ON/OFFなし方針はすべてTask 1/2に反映済み。
- Placeholder scan: プレースホルダー的な記述なし、全ステップに実コードを記載。
- Type consistency: `pickComposePlaceholder(date?: Date): string` の型はTask 1で定義し、Task 2で同一シグネチャのまま消費している。
