#!/usr/bin/env python3
"""VOICEVOX の VVM カタログ(`catalog.json`)を、上流のリリースから生成する(Issue #625 段階2)。

使い方: python3 -I scripts/gen-voicevox-catalog.py > src-tauri/src/tts/voicevox/catalog.json
VVM 本体は取得しない。zip の末尾だけを Range 取得して `metas.json` を読む。
読み上げに使えるスタイル(`talk` / `streaming_talk`)だけを入れ、歌声だけの VVM(`s0`)は除く。
"""
import io, json, re, sys, urllib.request, zipfile

REPO = "VOICEVOX/voicevox_vvm"
TAG = "0.17.1"
BASE = f"https://github.com/{REPO}/releases/download/{TAG}"
TALK_TYPES = {"talk", "streaming_talk"}
NEMO_CREDIT = "VOICEVOX Nemo"


def get(url: str, headers: dict | None = None, method: str = "GET"):
    req = urllib.request.Request(url, headers=headers or {}, method=method)
    return urllib.request.urlopen(req)


class Ranged(io.RawIOBase):
    """HTTP の Range で読む、シーク可能なファイル風オブジェクト。"""

    def __init__(self, url: str):
        self.url, self.pos = url, 0
        self.size = int(get(url, method="HEAD").headers["Content-Length"])

    def seekable(self): return True
    def readable(self): return True
    def tell(self): return self.pos

    def seek(self, off, whence=0):
        self.pos = off if whence == 0 else self.pos + off if whence == 1 else self.size + off
        return self.pos

    def read(self, n=-1):
        if n < 0:
            n = self.size - self.pos
        n = min(n, self.size - self.pos)
        if n <= 0:
            return b""
        data = get(self.url, {"Range": f"bytes={self.pos}-{self.pos + n - 1}"}).read()
        self.pos += len(data)
        return data


def credits(terms: str) -> dict[str, str]:
    """規約の `## キャラ名` の節から、「…」で示されたクレジット表記を取り出す。"""
    out = {}
    for name, body in re.findall(r"^## (.+?)\n(.*?)(?=^## |\Z)", terms, re.S | re.M):
        m = re.search(r"「([^」]+)」と[^。]{0,8}クレジット", body.replace("\n", ""))
        if m:
            out[name.strip()] = m.group(1)
    return out


def main() -> None:
    rel = json.load(get(f"https://api.github.com/repos/{REPO}/releases/tags/{TAG}"))
    assets = {a["name"]: a for a in rel["assets"]}
    terms = get(f"{BASE}/TERMS.txt").read().decode("utf-8")
    cred = credits(terms)
    vvms = []
    for name in sorted((n for n in assets if n.endswith(".vvm")), key=lambda n: (not n[:-4].isdigit(), int(n[:-4]) if n[:-4].isdigit() else 0, n)):
        z = zipfile.ZipFile(Ranged(f"{BASE}/{name}"))
        chars = []
        for m in json.loads(z.read("metas.json")):
            styles = [{"id": s["id"], "name": s["name"]} for s in m["styles"] if s["type"] in TALK_TYPES]
            if not styles:
                continue
            credit = cred.get(m["name"]) or (NEMO_CREDIT if name == "n0.vvm" else None)
            if credit is None:
                # 規約に個別の節がないキャラは、全体の規約(「VOICEVOX を利用したことがわかるクレジット表記」)に従う。
                credit = f"VOICEVOX:{m['name']}"
                print(f"警告: 規約に個別の節がない。既定のクレジットにした: {m['name']} ({name})", file=sys.stderr)
            chars.append({"name": m["name"], "credit": credit, "styles": styles})
        if not chars:
            continue
        digest = assets[name]["digest"]
        assert digest.startswith("sha256:"), digest
        vvms.append({"id": name[:-4], "size": assets[name]["size"], "sha256": digest[7:], "characters": chars})
    json.dump({"vvm_release": TAG, "vvms": vvms}, sys.stdout, ensure_ascii=False, indent=1)
    print()


if __name__ == "__main__":
    main()
