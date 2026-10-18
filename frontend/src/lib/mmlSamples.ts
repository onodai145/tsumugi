// MML 入力欄の「サンプル」ボタンが入れる例(Issue #500)。本体は mmlSampleData.json で、
// Rust 側のテスト(frontend_samples_parse_within_limits)が実際のパーサーと上限で通ることを確かめる。
import data from "./mmlSampleData.json";

export interface MmlSample {
  id: string;
  label: string;
  mml: string;
}

export const MML_SAMPLES: readonly MmlSample[] = data;
