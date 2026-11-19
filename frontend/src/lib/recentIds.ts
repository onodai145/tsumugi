// 直近の ID を一定数だけ覚えて、重複を弾く小さな集合(古い ID から忘れる)。
// 通知音・デスクトップ通知(`note:` / 通知 ID)と、読み上げ(Issue #625)が、互いの枠を食わないよう、
// 用途ごとに別のインスタンスを持つ。
export class RecentIds {
  #ids = new Set<string>();
  #order: string[] = [];
  #cap: number;

  constructor(cap: number) {
    this.#cap = cap;
  }

  /// 初めての ID なら覚えて true、すでに覚えている ID なら false。
  mark(id: string): boolean {
    if (this.#ids.has(id)) return false;
    this.#ids.add(id);
    this.#order.push(id);
    if (this.#order.length > this.#cap) {
      const old = this.#order.shift();
      if (old !== undefined) this.#ids.delete(old);
    }
    return true;
  }
}
