// @misskey-dev/emoji-assets（Misskey本家が使うのと同じ絵文字画像パッケージ）を
// public/ 配下へコピーする。Twemoji/Fluent Emojiをインスタンス配信やCDNに頼らず
// アプリに同梱し、ビルド後は完全オフラインで解決できるようにするため。
// node_modules 配下の巨大バイナリを毎回コピーし直さないよう、既に存在すればスキップする。
// 注意: @misskey-dev/emoji-assets を更新しても既存の public/twemoji, public/fluent-emoji は
// 自動更新されない。バージョンを上げたら `rm -rf public/twemoji public/fluent-emoji` してから
// 再度 install/dev/build すること。
import { existsSync, cpSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.dirname(fileURLToPath(import.meta.url));
const srcBase = path.join(root, "..", "node_modules", "@misskey-dev", "emoji-assets", "built");
const destBase = path.join(root, "..", "public");

for (const name of ["twemoji", "fluent-emoji"]) {
  const src = path.join(srcBase, name);
  const dest = path.join(destBase, name);
  if (existsSync(dest)) continue;
  cpSync(src, dest, { recursive: true });
  console.log(`[copy-emoji-assets] ${name} -> public/${name}`);
}
