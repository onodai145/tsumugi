# 日時入力(DateTimeInput)の共有化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Linux(WebKitGTK)でも日付と時刻を選べるよう、予約と投票締切の日時入力を flatpickr ベースの共有部品 `DateTimeInput` に置き換え、`SearchModal` の flatpickr 呼び出しとテーマ CSS を共有ファイルへ切り出す。

**Architecture:** `SearchModal` の `datePicker` アクションを `lib/flatpickrDatePicker.ts` へ、`:global` のテーマ CSS を `lib/flatpickrTheme.css` へ移す(挙動は変えない)。その上に、`datetime-local` と同じ文字列形式を値に持つラッパ `ui/DateTimeInput.svelte` を作り、`ComposeBar` の 2 か所のネイティブ input を置き換える。`ComposeBar` の既存ロジック(検証・送信・復元)は文字列形式が同じなので変えない。

**Tech Stack:** Svelte 5(runes)、flatpickr 4.6、Vitest + @testing-library/svelte、Vite。

**Spec:** `docs/superpowers/specs/2026-10-08-datetime-input-design.md`

## Global Constraints

- ネイティブの `datetime-local` / `date` / `time` input は使わない(WebKitGTK のネイティブ UI は未成熟。`SearchModal.svelte` の既存コメントと Issue #430 の診断)。
- `DateTimeInput` の `value` は `"YYYY-MM-DDTHH:mm"`(ローカルタイムゾーン、`<input type="datetime-local">` と同じ形式)。空は `""`。表示形式は `Y-m-d H:i`。
- flatpickr の設定は `SearchModal` の現状のまま: `enableTime: true`、`time_24hr: true`、`dateFormat: "Y-m-d H:i"`、日本語ロケール、`onReady` での年→月の入れ替え。`disableMobile` は指定しない。
- `SearchModal.test.ts` は **変更しない**。変更が必要になったら、それ自体が回帰の兆候なので、手を入れずに報告する。
- テーマ CSS の値(カラートークン、`appearance: none` の理由コメント)は変えない。`flatpickr.min.css` → テーマ CSS の読み込み順を保つ。
- `ComposeBar.svelte` は、2 つの input の置き換えと import 追加以外のロジックを変えない。
- コミットメッセージは件名のみ(本文なし)。末尾の Co-Authored-By トレーラは別途付与される。`--no-verify` / `--no-gpg-sign` は使わない。コミットが失敗またはタイムアウトしたら、リトライせず停止して報告する。
- `frontend/src/bindings/tauri.gen.ts` は触らない。Rust は変更しない。
- フロントのテストは `cd frontend && npx vitest run <path>`(`pnpm test -- <path>` はパスを無視する)。`pnpm check` はエラー 0・既存の警告 1(UrlPreviewCard)のまま保つ。
- 実 UI の確認は Xvfb 越しで行い(`WAYLAND_DISPLAY` は unset)、起動したプロセスは PID 指定で止める(`pkill`/`killall` 禁止)。

## Review Focus

1. 日付が **ローカルタイムゾーン** で `value` になる(UTC に変換されない)。0 時台の日時は UTC 変換で前日にずれるので、そこを固定する(Task 2)。
2. `value` を外から変えたとき、表示だけが追従し、`onChange` は発火せず `value` が書き戻されない(無限ループや、値の上書きが起きない)(Task 2)。
3. `value` を `""` にしたとき(予約成功後の消去、解除、投票締切の下書き復元)に、表示も空になる(Task 2・3)。
4. テーマ CSS の移動でルールが欠けない(Task 1)。
5. 「作成欄に戻す」で予約日時が復元されたとき、入力欄の表示が復元した日時になる(Task 3)。

---

### Task 0: 原因の確認(controller が実施。コード変更なし)

**Files:** なし(作業ファイルは scratchpad に置き、コミットしない)

目的: 仮説(WebKitGTK のネイティブ `datetime-local` は時刻を操作できない)を、この環境の WebKitGTK 2.52 で再現して確かめる。再現しなければ、原因の仮説を見直し、以降のタスクに進まずユーザーに報告する。

- [ ] **Step 1: 最小の HTML を WebKitGTK で開き、ポップアップを撮る**

```bash
S=/tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/87f19ff2-d123-4b87-8304-7ea2879f7256/scratchpad
cat > $S/native.html <<'EOF'
<!doctype html><meta charset=utf-8>
<body style="font:20px sans-serif;margin:30px">
<input type="datetime-local" id="a" value="2026-10-08T09:30" style="font-size:20px;width:300px">
</body>
EOF
cat > $S/native.py <<'EOF'
import gi, sys
gi.require_version("Gtk", "3.0"); gi.require_version("WebKit2", "4.1")
from gi.repository import Gtk, WebKit2, GLib
w = Gtk.Window(); w.set_default_size(700, 500); w.move(0, 0)
v = WebKit2.WebView(); w.add(v); v.load_uri("file://" + sys.argv[1]); w.show_all()
GLib.timeout_add_seconds(int(sys.argv[2]), Gtk.main_quit)
w.connect("destroy", Gtk.main_quit); Gtk.main()
EOF
Xvfb :97 -screen 0 1024x768x24 >/dev/null 2>&1 &
echo $! > $S/xvfb97.pid
sleep 1
DISPLAY=:97 env -u WAYLAND_DISPLAY python3 $S/native.py $S/native.html 12 >$S/native.log 2>&1 &
echo $! > $S/native.pid
sleep 4
# 入力欄の中(日付部分→カレンダーボタン付近)をクリックしてポップアップを出す
DISPLAY=:97 xdotool mousemove 120 60 click 1
sleep 2
DISPLAY=:97 import -window root $S/native-popup.png
```

- [ ] **Step 2: 画像を見て判断する**

`Read` で `$S/native-popup.png` を開く。ポップアップに「時刻を選ぶ手段(時・分のスピナーや一覧)」があるか確認する。
- 時刻の手段が **ない**(カレンダーのみ): 仮説を支持する。Step 3 へ。
- 時刻の手段が **ある**: 仮説は再現しない。ここで止まり、ユーザーに「この環境の WebKitGTK 2.52 では再現しない。報告された環境のバージョンや、入力欄の操作方法(時・分のセグメントをキーボードで入力できるか)を確認したい」と報告する。以降のタスクは進めない。
- クリック位置がずれてポップアップが出ない場合: `xdotool getmouselocation` と撮った画像で入力欄の位置を確かめ、座標を調整して撮り直す(最大 3 回)。それでも出なければ、その旨を報告して止まる。

- [ ] **Step 3: 後始末(PID 指定)**

```bash
kill $(cat $S/native.pid) $(cat $S/xvfb97.pid) 2>/dev/null; rm -f $S/*.pid
```

---

### Task 1: flatpickr の呼び出しとテーマ CSS を共有ファイルへ切り出す

**Files:**
- Create: `frontend/src/lib/flatpickrDatePicker.ts`
- Create: `frontend/src/lib/flatpickrDatePicker.test.ts`
- Create: `frontend/src/lib/flatpickrTheme.css`(`SearchModal.svelte` の `<style>` から生成)
- Modify: `frontend/src/ui/SearchModal.svelte`(アクション・flatpickr の import・`<style>` を削除し、共有モジュールを import)

**Interfaces:**
- Produces: `export function datePicker(node: HTMLInputElement, opts: DatePickerOptions)`、`export type DatePickerOptions = { defaultHour: number; defaultMinute?: number; onChange: (d: Date | null) => void; onCreate: (fp: FlatpickrInstance) => void }`(Task 2 が使う)。`defaultMinute` 未指定は `defaultHour === 0 ? 0 : 59`(`SearchModal` の現状の規則)。

- [ ] **Step 1: 移動前のビルド結果を控える(CSS の欠落を後で検出するため)**

```bash
cd frontend && pnpm build >/dev/null 2>&1; echo build=$?
cat dist/assets/*.css | grep -o 'flatpickr[a-zA-Z0-9_.-]*' | sort | uniq -c | md5sum | tee /tmp/css-before.md5
cat dist/assets/*.css | grep -c 'color-popover' | tee /tmp/css-before.count
```
Expected: `build=0`、md5 とカウントが出る(後で比較する)。

- [ ] **Step 2: 失敗するテストを書く**

`frontend/src/lib/flatpickrDatePicker.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";
import { datePicker } from "./flatpickrDatePicker";

function mount(over: { defaultHour?: number; defaultMinute?: number } = {}) {
  const node = document.createElement("input");
  document.body.appendChild(node);
  let fp!: FlatpickrInstance;
  const onChange = vi.fn();
  const action = datePicker(node, {
    defaultHour: 0,
    onChange,
    onCreate: (f) => (fp = f),
    ...over,
  });
  return { node, fp, onChange, action };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("datePicker", () => {
  it("onCreate でインスタンスを渡し、24時間制・日本語・Y-m-d H:i で作る", () => {
    const { fp } = mount();
    expect(fp.config.enableTime).toBe(true);
    expect(fp.config.time_24hr).toBe(true);
    expect(fp.config.dateFormat).toBe("Y-m-d H:i");
    expect(fp.l10n.weekdays.shorthand[0]).toBe("日");
  });

  it("defaultMinute 未指定は、defaultHour が 0 なら 0、それ以外は 59(検索の開始/終了の既存規則)", () => {
    expect(mount({ defaultHour: 0 }).fp.config.defaultMinute).toBe(0);
    expect(mount({ defaultHour: 23 }).fp.config.defaultMinute).toBe(59);
  });

  it("defaultMinute を指定するとその値になる", () => {
    const { fp } = mount({ defaultHour: 9, defaultMinute: 30 });
    expect(fp.config.defaultHour).toBe(9);
    expect(fp.config.defaultMinute).toBe(30);
  });

  it("日付を選ぶと onChange に Date が、クリアすると null が渡る", () => {
    const { fp, onChange } = mount();
    const d = new Date(2026, 0, 2, 3, 4);
    fp.setDate(d, true);
    expect(onChange).toHaveBeenLastCalledWith(d);
    fp.clear();
    expect(onChange).toHaveBeenLastCalledWith(null);
  });

  it("destroy でカレンダー要素が body から消える", () => {
    const { action } = mount();
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(1);
    action.destroy();
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(0);
  });
});
```

Run: `cd frontend && npx vitest run src/lib/flatpickrDatePicker.test.ts`
Expected: FAIL(`./flatpickrDatePicker` が見つからない)

- [ ] **Step 3: テーマ CSS を生成する**

`SearchModal.svelte` の `<style>`(434〜540 行)から、`:global(...)` のラッパを外した `.css` を作る。ラッパ内に括弧を含むセレクタ(`:nth-child(7n + 1)`、複数行の `:global(\n ... \n)`)があるので、括弧の対応を数えるスクリプトで外す。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
python3 - <<'EOF'
import re
src = open("frontend/src/ui/SearchModal.svelte", encoding="utf-8").read().split("\n")
assert src[432] == "<style>" and src[540] == "</style>", (src[432], src[540])
block = src[433:540]                      # <style> の内側(434〜540行)
text = "\n".join(l[2:] if l.startswith("  ") else l for l in block) + "\n"
out, i = [], 0
while i < len(text):
    if text.startswith(":global(", i):
        j = i + len(":global("); depth, k = 1, j
        while depth:
            c = text[k]; depth += (c == "("); depth -= (c == ")"); k += 1
        out.append(re.sub(r"\s*\n\s*", " ", text[j:k-1]).strip())
        i = k
    else:
        out.append(text[i]); i += 1
css = "".join(out)
assert ":global(" not in css
open("frontend/src/lib/flatpickrTheme.css", "w", encoding="utf-8").write(css)
print("rules before:", text.count("{"), "after:", css.count("{"))
EOF
```
Expected: `rules before: N after: N`(同数)。

先頭のコメント(「効かずすべて:globalが必要」と書かれた 2 行)は、CSS ファイルでは意味が変わるので、次の内容に書き換える。

```css
/* flatpickr のカレンダーは input の外(通常 body 直下)に生成される。app.css のカラートークンに載せ替えて
   ライト/ダーク両対応にする。WebKitGTK のネイティブ描画の癖への対処を含むため、SearchModal と
   DateTimeInput で共有する(Issue #430, #60)。 */
```

- [ ] **Step 4: アクションを共有モジュールにする**

`frontend/src/lib/flatpickrDatePicker.ts`:

```ts
// flatpickr(自前描画のカレンダー)を素の input に被せるアクション。
// WebKitGTK のネイティブ datetime-local/date/time は UI が未成熟(時刻を操作できない等)なため、
// 日時入力はネイティブ input ではなくこれを使う(Issue #430, #60)。
import flatpickr from "flatpickr";
import "flatpickr/dist/flatpickr.min.css";
import "./flatpickrTheme.css"; // flatpickr.min.css より後に読み込む(同じ詳細度のルールを上書きするため)
import { Japanese } from "flatpickr/dist/l10n/ja.js";
import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";

export type DatePickerOptions = {
  /// 日付だけ選んで時刻を触らなかったときの時。
  defaultHour: number;
  /// 同じく分。未指定は、defaultHour が 0 なら 0、それ以外は 59(検索の開始=0:00、終了=その日の終わり)。
  defaultMinute?: number;
  onChange: (d: Date | null) => void;
  /// fp インスタンスは、クリアボタン(fp.clear())や値の外部更新(fp.setDate())から使えるよう返す。
  onCreate: (fp: FlatpickrInstance) => void;
};

export function datePicker(node: HTMLInputElement, opts: DatePickerOptions) {
  const fp: FlatpickrInstance = flatpickr(node, {
    enableTime: true,
    time_24hr: true,
    dateFormat: "Y-m-d H:i",
    locale: Japanese,
    defaultHour: opts.defaultHour,
    defaultMinute: opts.defaultMinute ?? (opts.defaultHour === 0 ? 0 : 59),
    onChange: (dates) => opts.onChange(dates[0] ?? null),
    // flatpickr本体はヘッダを常に「月セレクト→年input」の順でDOM生成し、これを入れ替える
    // 設定は無い。buildMonths()自体は初期化時に1度しか呼ばれない(月送りは値の更新のみ)
    // ので、onReadyで年側のwrapperを月セレクトの前に差し替えれば以後も維持される。
    onReady: (_selectedDates, _dateStr, instance) => {
      const yearWrapper = instance.currentYearElement.closest(".numInputWrapper");
      const monthSelect = instance.monthsDropdownContainer;
      if (yearWrapper && monthSelect?.parentElement) {
        monthSelect.parentElement.insertBefore(yearWrapper, monthSelect);
      }
    },
  });
  opts.onCreate(fp);
  return {
    destroy() {
      fp.destroy();
    },
  };
}
```

- [ ] **Step 5: `SearchModal.svelte` を共有モジュールに切り替える**

スクリプトで機械的に消す(手作業の行番号ずれを避ける)。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
python3 - <<'EOF'
p = "frontend/src/ui/SearchModal.svelte"
s = open(p, encoding="utf-8").read()
def cut(s, start, end, keep_end=True):
    assert s.count(start) == 1 and s.count(end) == 1, (start, end)
    a, b = s.index(start), s.index(end)
    assert a < b
    return s[:a] + s[b:]
# 1) ローカルの datePicker アクション(直前のコメントごと)。次の `let notes` の手前まで。
s = cut(s, "  // flatpickrをSvelteのバインディングなしに素のinputへ被せるアクション。", "  let notes = $state<Note[]>([]);")
# 2) <style> ブロック全体(末尾まで)
a = s.index("\n<style>\n"); assert s.count("<style>") == 1
s = s[:a].rstrip("\n") + "\n"
# 3) flatpickr 本体と CSS・ロケールの import を消し、共有アクションの import を足す(型 import は残す)
for line in ['  import flatpickr from "flatpickr";\n',
             '  import "flatpickr/dist/flatpickr.min.css";\n',
             '  import { Japanese } from "flatpickr/dist/l10n/ja.js";\n']:
    assert s.count(line) == 1, line
    s = s.replace(line, "")
anchor = '  import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";\n'
assert s.count(anchor) == 1
s = s.replace(anchor, anchor + '  import { datePicker } from "../lib/flatpickrDatePicker";\n')
open(p, "w", encoding="utf-8").write(s)
EOF
git diff --stat frontend/src/ui/SearchModal.svelte
grep -n "flatpickr\|datePicker\|<style" frontend/src/ui/SearchModal.svelte | cut -c1-120
```
Expected: `SearchModal.svelte` から約 150 行が消える。残る `flatpickr` の言及は、型 import、新しい `datePicker` の import、使用箇所のコメント(`flatpickr（自前描画…`)、`use:datePicker`、`dateFromFp`/`dateToFp` のみ。`<style` は 0 件。

- [ ] **Step 6: テストとビルドの同値確認**

```bash
cd frontend
npx vitest run src/lib/flatpickrDatePicker.test.ts src/ui/SearchModal.test.ts
pnpm build >/dev/null 2>&1; echo build=$?
cat dist/assets/*.css | grep -o 'flatpickr[a-zA-Z0-9_.-]*' | sort | uniq -c | md5sum
cat /tmp/css-before.md5
cat dist/assets/*.css | grep -c 'color-popover'; cat /tmp/css-before.count
pnpm check 2>&1 | tail -2
```
Expected: 両テストファイルが PASS(`SearchModal.test.ts` は無変更)。`build=0`。移動前後の md5 とカウントが **一致**(テーマ CSS のルールが欠けていない)。`pnpm check` は 0 errors / 1 warning。md5 が一致しない場合は、Step 3 のスクリプトの結果(ルール数)と比べて原因を探し、報告する。

- [ ] **Step 7: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/flatpickrDatePicker.ts frontend/src/lib/flatpickrDatePicker.test.ts frontend/src/lib/flatpickrTheme.css frontend/src/ui/SearchModal.svelte
git commit -m "refactor: flatpickrの呼び出しとテーマCSSをSearchModalから共有ファイルへ切り出す(#60)"
```

---

### Task 2: `DateTimeInput.svelte`

**Files:**
- Create: `frontend/src/ui/DateTimeInput.svelte`
- Create: `frontend/src/ui/DateTimeInputHarness.svelte`(テスト用のホスト。`value` の書き戻しを観測するため)
- Create: `frontend/src/ui/DateTimeInput.test.ts`

**Interfaces:**
- Consumes: Task 1 の `datePicker`、`lib/schedule.ts` の `epochSecToLocalInput(sec: number): string`
- Produces: `<DateTimeInput bind:value defaultHour? defaultMinute? placeholder? class? disabled? data-testid? />`(Task 3 が使う)。`data-testid` は入力欄の `<input>` 要素に付く。

- [ ] **Step 1: テスト用ホストと失敗するテストを書く**

`frontend/src/ui/DateTimeInputHarness.svelte`:

```svelte
<script lang="ts">
  // DateTimeInput.test.ts 用のホスト。bind:value の書き戻しを <output> で観測し、
  // setValue() で外からの更新(予約成功後の消去・復元など)を再現する。
  import DateTimeInput from "./DateTimeInput.svelte";

  let {
    initial = "",
    defaultHour,
    defaultMinute,
  }: { initial?: string; defaultHour?: number; defaultMinute?: number } = $props();

  let value = $state(initial);
  export function setValue(v: string) {
    value = v;
  }
</script>

<DateTimeInput bind:value {defaultHour} {defaultMinute} data-testid="dt" />
<output data-testid="bound">{value}</output>
```

`frontend/src/ui/DateTimeInput.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { tick } from "svelte";
import Harness from "./DateTimeInputHarness.svelte";

type Fp = { _flatpickr: { setDate(d: Date, t: boolean): void; selectedDates: Date[] } };
const fpOf = (el: HTMLElement) => (el as unknown as Fp)._flatpickr;

afterEach(() => cleanup());

describe("DateTimeInput", () => {
  it("ネイティブの datetime-local ではなく、読み取り専用のテキスト入力を使う", () => {
    const { getByTestId } = render(Harness);
    const input = getByTestId("dt") as HTMLInputElement;
    expect(input.type).toBe("text");
    expect(input.readOnly).toBe(true);
  });

  it("初期値を Y-m-d H:i で表示する", () => {
    const { getByTestId } = render(Harness, { props: { initial: "2026-03-04T05:06" } });
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("2026-03-04 05:06");
  });

  // Review Focus 1: UTC ではなくローカルの成分で value を作る(0 時台は UTC 変換すると前日にずれる)
  it("選んだ日時をローカルタイムゾーンの YYYY-MM-DDTHH:mm で value に書き戻す", async () => {
    const { getByTestId } = render(Harness);
    fpOf(getByTestId("dt")).setDate(new Date(2026, 0, 1, 0, 30), true);
    await tick();
    expect(getByTestId("bound").textContent).toBe("2026-01-01T00:30");
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("2026-01-01 00:30");
  });

  // Review Focus 2: 外からの更新は表示にだけ反映し、書き戻しやループを起こさない
  it("value を外から変えると表示が追従し、value は変わらない", async () => {
    const { getByTestId, component } = render(Harness);
    component.setValue("2026-10-08T09:30");
    await tick();
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("2026-10-08 09:30");
    expect(getByTestId("bound").textContent).toBe("2026-10-08T09:30");
    expect(fpOf(getByTestId("dt")).selectedDates).toHaveLength(1);
  });

  // Review Focus 3
  it("value を空にすると表示も空になる", async () => {
    const { getByTestId, component } = render(Harness, { props: { initial: "2026-03-04T05:06" } });
    component.setValue("");
    await tick();
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("");
    expect(fpOf(getByTestId("dt")).selectedDates).toHaveLength(0);
    expect(getByTestId("bound").textContent).toBe("");
  });

  it("アンマウントでカレンダー要素が body から消える", () => {
    const { unmount } = render(Harness);
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(1);
    unmount();
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(0);
  });
});
```

Run: `cd frontend && npx vitest run src/ui/DateTimeInput.test.ts`
Expected: FAIL(`DateTimeInput.svelte` が見つからない)

- [ ] **Step 2: 実装する**

`frontend/src/ui/DateTimeInput.svelte`:

```svelte
<script lang="ts">
  // 日時入力。ネイティブの <input type="datetime-local"> は WebKitGTK で時刻を操作できない
  // (Issue #430 で診断済み)ため、flatpickr(自前描画)で置き換える。
  // value は datetime-local と同じ "YYYY-MM-DDTHH:mm"(ローカルタイムゾーン)、空は ""。
  import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";
  import { datePicker } from "../lib/flatpickrDatePicker";
  import { epochSecToLocalInput } from "../lib/schedule";

  let {
    value = $bindable(""),
    defaultHour = 9,
    defaultMinute = 0,
    placeholder = "",
    class: className = "",
    disabled = false,
    "data-testid": testid,
  }: {
    value?: string;
    defaultHour?: number;
    defaultMinute?: number;
    placeholder?: string;
    class?: string;
    disabled?: boolean;
    "data-testid"?: string;
  } = $props();

  let fp: FlatpickrInstance | undefined;

  const format = (d: Date) => epochSecToLocalInput(Math.floor(d.getTime() / 1000));
  const shownValue = () => (fp?.selectedDates[0] ? format(fp.selectedDates[0]) : "");

  // 外から value が変わったときだけ、表示を合わせる。setDate(.., false) は onChange を発火させないので
  // value を書き戻さない。表示と同じ値なら何もしない(ループ・二重更新を避ける)。
  $effect(() => {
    const v = value;
    if (!fp || v === shownValue()) return;
    fp.setDate(v ? new Date(v) : [], false);
  });

  function onCreate(f: FlatpickrInstance) {
    fp = f;
    if (value) f.setDate(new Date(value), false);
  }
</script>

<input
  type="text"
  readonly
  {placeholder}
  {disabled}
  class={className}
  data-testid={testid}
  use:datePicker={{
    defaultHour,
    defaultMinute,
    onChange: (d) => (value = d ? format(d) : ""),
    onCreate,
  }}
/>
```

- [ ] **Step 3: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/DateTimeInput.test.ts src/lib/flatpickrDatePicker.test.ts && pnpm check 2>&1 | tail -2`
Expected: PASS(6 tests + 5 tests)。`pnpm check` は 0 errors / 1 warning。`$effect` が初回に `fp` 未設定で何もしない、アクションの `onCreate` で初期値を入れる、の両方で「初期値を表示する」テストが通ること。通らない場合は、アクションが `$effect` より後に走る順序を疑い、`onCreate` の初期化を正としたまま原因を報告する(`$effect` 側に初期化を寄せない)。

- [ ] **Step 4: コミット**

```bash
git add frontend/src/ui/DateTimeInput.svelte frontend/src/ui/DateTimeInputHarness.svelte frontend/src/ui/DateTimeInput.test.ts
git commit -m "feat: flatpickrベースの日時入力DateTimeInputを追加(#60)"
```

---

### Task 3: `ComposeBar` の 2 か所を置き換える

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`(import 追加、`~1085` 行の投票締切、`~1152` 行の予約日時)
- Modify: `frontend/src/ui/ComposeBar.test.ts`(予約・投票締切の日時入力の操作と表示値の期待)

**Interfaces:**
- Consumes: Task 2 の `DateTimeInput`(`bind:value`、`class`、`data-testid`、`defaultHour`、`placeholder`)
- Produces: testid `compose-schedule-input`(予約の入力欄、従来どおり)、`compose-poll-expires-at`(投票締切の入力欄、新規)

- [ ] **Step 1: テストを新しい入力欄に合わせて書き換える(RED)**

`frontend/src/ui/ComposeBar.test.ts`:

(a) import に追加: `import { tick } from "svelte";`(既存の import の並びに合わせる)。

(b) `describe("ComposeBar 予約投稿", () => {` の直前に、ヘルパを足す。

```ts
// 日時入力は flatpickr(読み取り専用のテキスト入力)なので、fireEvent.input ではなく
// flatpickr のインスタンスに日時を入れる。値は datetime-local 形式 "YYYY-MM-DDTHH:mm"。
type FlatpickrHost = { _flatpickr: { setDate(d: Date, triggerChange: boolean): void } };
async function setDateTime(el: HTMLElement, value: string) {
  (el as unknown as FlatpickrHost)._flatpickr.setDate(new Date(value), true);
  await tick();
}
// 入力欄の表示値は "YYYY-MM-DD HH:mm"(flatpickr の dateFormat)。
const toShown = (localInput: string) => localInput.replace("T", " ");
```

(c) 予約の日時を `fireEvent.input` で入れている箇所を、すべて `setDateTime` に置き換える。値の書き方が 2 通り(`{ value }` と `{ value: EXPR }`)あるのでスクリプトで直す。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
python3 - <<'EOF'
import re
p = "frontend/src/ui/ComposeBar.test.ts"
s = open(p, encoding="utf-8").read()
pat = re.compile(r'await fireEvent\.input\(((?:\w+\.)?getByTestId\("compose-schedule-input"\)), \{ target: \{ value(?:: ([^}]+?))? \} \}\);')
def sub(m):
    el, expr = m.group(1), m.group(2) or "value"
    return f"await setDateTime({el}, {expr});"
s, n = pat.subn(sub, s)
print("replaced", n)
open(p, "w", encoding="utf-8").write(s)
EOF
grep -n 'fireEvent.input(.*compose-schedule-input' frontend/src/ui/ComposeBar.test.ts || echo "none left"
```
Expected: `replaced 8` 前後(置き換え数が 0 ならパターンが合っていないので、`grep -n 'compose-schedule-input' frontend/src/ui/ComposeBar.test.ts` で実際の書き方を見て正規表現を直す)。`none left`。

(d) 投票締切(日時指定)のテスト(「日時指定の投票の締切が予約日時以前ならエラーを出し…」)の入力部分を置き換える。

```ts
    await fireEvent.input(ui.container.querySelector("input[type=datetime-local]:not([data-testid])") as HTMLInputElement, {
      target: { value: before },
    });
```
を、次にする。

```ts
    await setDateTime(ui.getByTestId("compose-poll-expires-at"), before);
```

(e) 「未来の予約を作成欄に戻すと予約日時も復元され…」の期待を、表示値の形式に直す。

```ts
      expect((ui.getByTestId("compose-schedule-input") as HTMLInputElement).value).toBe(
        epochSecToLocalInput(futureSec),
      ),
```
を、次にする。

```ts
      expect((ui.getByTestId("compose-schedule-input") as HTMLInputElement).value).toBe(
        toShown(epochSecToLocalInput(futureSec)),
      ),
```
(「過去を戻しても復元しない」の `toBe("")` はそのまま。)

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts -t "予約"`
Expected: FAIL(`_flatpickr` が undefined — まだネイティブ input のため)

- [ ] **Step 2: `ComposeBar.svelte` を置き換える**

(a) import を足す(既存の `ScheduledModal` の import の次):

```ts
  import DateTimeInput from "./DateTimeInput.svelte";
```

(b) 投票締切の入力(`pollExpiryMode === "at"` の分岐内)を置き換える。

```svelte
          <input type="datetime-local" bind:value={pollExpiresAt} class="rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-sm text-foreground" />
```
を、次にする。

```svelte
          <DateTimeInput
            bind:value={pollExpiresAt}
            placeholder="締切日時"
            data-testid="compose-poll-expires-at"
            class="w-40 rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-sm text-foreground"
          />
```

(c) 予約の日時入力を置き換える。

```svelte
          <input
            type="datetime-local"
            bind:value={scheduleAt}
            data-testid="compose-schedule-input"
            class="rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-sm text-foreground"
          />
```
を、次にする。

```svelte
          <DateTimeInput
            bind:value={scheduleAt}
            placeholder="予約日時"
            data-testid="compose-schedule-input"
            class="w-40 rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-sm text-foreground"
          />
```

他のロジックは変えない(`scheduleAt` / `pollExpiresAt` は従来どおり `T` 区切りの文字列)。

- [ ] **Step 3: テストが通ることを確認する**

Run:
```bash
cd frontend
npx vitest run src/ui/ComposeBar.test.ts src/ui/ComposeBar.haptics.test.ts src/ui/DateTimeInput.test.ts src/ui/SearchModal.test.ts
pnpm test 2>&1 | tail -6
pnpm check 2>&1 | tail -2
grep -n 'type="datetime-local"\|type="date"\|type="time"' src -r --include='*.svelte' || echo "native date inputs: none"
```
Expected: すべて PASS(全体で 700 件超、失敗 0)。`pnpm check` は 0 errors / 1 warning。`native date inputs: none`。ここで落ちるテストがあれば、テストの意図(過去日時の拒否、期間指定の基準、復元、解除)を変えずに、入力の操作と表示値の期待だけを直す。意図を変えないと通らない場合は、実装の問題として報告する。

- [ ] **Step 4: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "fix: 予約と投票締切の日時入力をflatpickrに置き換えてLinuxでも時刻を選べるようにする(#60)"
```

---

### Task 4: ドキュメントと最終確認

**Files:**
- Modify: `docs/design/style-guide.md`(「日時入力」の節を足す)
- Modify: `CLAUDE.md`(frontend の節に 1 行)

- [ ] **Step 1: スタイルガイドに節を足す**

`docs/design/style-guide.md` の `## 12. 今後` の直前に、次の節を足し、`## 12. 今後` を `## 13. 今後` にする(`grep -n "12\." docs/design/style-guide.md` で他の参照が無いことを確かめる)。

```markdown
## 12. 日時入力

ネイティブの `<input type="datetime-local">` / `date` / `time` は使わない。WebKitGTK(Linux)のネイティブ UI は未成熟で、時刻を操作できない(Issue #430、予約投稿 #60 で確認)。

- フォームの日時入力は `ui/DateTimeInput.svelte` を使う。値は `datetime-local` と同じ `"YYYY-MM-DDTHH:mm"`(ローカルタイムゾーン)、空は `""`。
- `Date` オブジェクトで扱う場合(検索モーダルの日時範囲)は `lib/flatpickrDatePicker.ts` の `datePicker` アクションを使う。
- カレンダーのテーマは `lib/flatpickrTheme.css` に集約している。個別に上書きしない。
```

- [ ] **Step 2: `CLAUDE.md` に 1 行足す**

`CLAUDE.md` の「### frontend/src layout」の節の、`WebKitGTK layout quirk (Issue #166)` の段落の直前に追加する。

```markdown
WebKitGTK's native `<input type="datetime-local">` / `date` / `time` can't be used to pick a time (Issue #430, #60): use `ui/DateTimeInput.svelte` (or the `datePicker` action in `lib/flatpickrDatePicker.ts` when you need a `Date`) instead of a native date/time input — see `docs/design/style-guide.md` §12.
```

- [ ] **Step 3: 全体の自動検証**

Run:
```bash
cd frontend && pnpm test 2>&1 | tail -5 && pnpm check 2>&1 | tail -2 && pnpm build >/dev/null 2>&1; echo build=$?
cd ../src-tauri && cargo test 2>&1 | grep -E "^test result" | head -1
cd .. && git status --short
```
Expected: フロントの全テスト PASS、`pnpm check` 0 errors / 1 warning、`build=0`、Rust は無変更で 581 passed、`git status` はクリーン(`tauri.gen.ts` が変わっていないこと)。

- [ ] **Step 4: コミット**

```bash
git add docs/design/style-guide.md CLAUDE.md
git commit -m "docs: 日時入力はネイティブinputを使わずDateTimeInputを使う旨を追記(#60)"
```

- [ ] **Step 5: 実機確認(controller がユーザーと実施。実装者は行わない)**

1. Task 0 と同じ手順で、`DateTimeInput` を載せた最小ページ、または実アプリを Xvfb 越しに開き、日付と時刻の両方を選べることを撮影して確認する。
2. ユーザーの実環境(Linux)で、予約の日時入力と、投票の「日時を指定」で、日付と時刻が選べることを確認してもらう。
3. 検索モーダルの日時範囲が従来どおり動くことを、同じ環境で確認してもらう(リファクタの回帰確認)。
