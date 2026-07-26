#!/usr/bin/env python3
"""release published イベントに合わせて packaging/aur/*/PKGBUILD の
pkgver・sha256sums を実測値に更新する。TAG 環境変数(例: v0.6.2)が必須。
"""
import hashlib
import os
import re
import urllib.request

TAG = os.environ["TAG"]
VER = TAG.lstrip("v")
REPO = "https://github.com/onodai145/tsumugi"


def sha256_url(url: str) -> str:
    with urllib.request.urlopen(url) as resp:
        return hashlib.sha256(resp.read()).hexdigest()


def update(path: str, urls: list[str]) -> None:
    with open(path) as f:
        content = f.read()

    content = re.sub(r"^pkgver=.*$", f"pkgver={VER}", content, flags=re.M)
    content = re.sub(r"^pkgrel=.*$", "pkgrel=1", content, flags=re.M)

    sums = [sha256_url(u) for u in urls]
    block = "sha256sums=(" + "\n            ".join(f"'{s}'" for s in sums) + ")"
    content = re.sub(r"sha256sums=\([^)]*\)", lambda _: block, content, flags=re.S)

    with open(path, "w") as f:
        f.write(content)


update(
    "packaging/aur/tsumugi/PKGBUILD",
    [f"{REPO}/archive/refs/tags/{TAG}.tar.gz"],
)
update(
    "packaging/aur/tsumugi-bin/PKGBUILD",
    [
        f"{REPO}/releases/download/{TAG}/tsumugi_{VER}_amd64.AppImage",
        f"{REPO}/raw/{TAG}/LICENSE",
    ],
)
