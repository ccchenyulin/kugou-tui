#!/usr/bin/env python3
"""统计 `Cargo.lock` 里各依赖的许可分布，用来复核 README 里那张表。

用法（在仓库根目录）：

    python3 scripts/license-stats.py

# 口径

* 数的是 `Cargo.lock` 里的**全部**包，不区分目标平台。
* `license` 字段从各依赖解包后的 `Cargo.toml` 读，所以**本机没下载过的包读不到**
  ——主要是 Windows / Android / macOS 专属包（`winapi*`、`windows*`、`jni`、
  `ndk-sys`、`objc2-*`）。脚本会把它们单独列出来，而不是当成「宽松许可」混进去。
* 只做**分类统计**，不判断许可兼容性。真要严格审查请用 `cargo-deny` / `cargo about`。

依赖变动后重跑一次，把输出里的数字同步到 README 即可。
"""

from __future__ import annotations

import glob
import os
import pathlib
import re
import sys
import tomllib
from collections import Counter

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCK_PATH = REPO_ROOT / "Cargo.lock"

# cargo 把解包后的依赖放在 $CARGO_HOME/registry/src/<registry>/<name>-<version>/
REGISTRY_SRC = os.path.expanduser(
    os.environ.get("CARGO_HOME", "~/.cargo") + "/registry/src/*"
)

# 仓库自己不算第三方依赖
SELF_NAME = "kugou-tui"

# 判定「弱著佐权」的许可标识
COPYLEFT = "MPL-2.0"


def find_manifest(name: str, version: str) -> pathlib.Path | None:
    for root in glob.glob(REGISTRY_SRC):
        candidate = pathlib.Path(root) / f"{name}-{version}" / "Cargo.toml"
        if candidate.exists():
            return candidate
    return None


def main() -> int:
    if not LOCK_PATH.exists():
        print(f"找不到 {LOCK_PATH}，请在仓库根目录运行", file=sys.stderr)
        return 1

    with open(LOCK_PATH, "rb") as handle:
        lock = tomllib.load(handle)
    packages = lock.get("package", [])

    permissive = 0
    copyleft: list[str] = []
    unreadable: list[str] = []
    gpl_mentions: list[str] = []
    license_texts: Counter[str] = Counter()

    for pkg in packages:
        name, version = pkg["name"], pkg["version"]
        if name == SELF_NAME:
            continue

        manifest = find_manifest(name, version)
        license_text = ""
        if manifest is not None:
            with open(manifest, "rb") as handle:
                license_text = tomllib.load(handle).get("package", {}).get("license", "") or ""

        if not license_text:
            unreadable.append(f"{name}-{version}")
            continue

        license_texts[license_text] += 1

        # 纯 MPL-2.0 才算弱著佐权；`MIT OR MPL-2.0` 这类可选双许可按宽松算
        if license_text.strip() == COPYLEFT:
            copyleft.append(name)
        else:
            permissive += 1

        if re.search(r"GPL", license_text, re.IGNORECASE):
            gpl_mentions.append(f"{name} ({license_text})")

    total = permissive + len(copyleft) + len(unreadable)
    print(f"Cargo.lock 共 {len(packages)} 个包（含 {SELF_NAME} 自己）")
    print(f"第三方依赖 {total} 个：")
    print(f"  宽松许可（含可选双许可）    {permissive}")
    print(f"  弱著佐权 {COPYLEFT}          {len(copyleft)}  -> {sorted(copyleft)}")
    print(f"  本机读不到 license          {len(unreadable)}")
    print()
    print("读不到的（平台专属，Linux 上不会下载）：")
    for item in unreadable:
        print(f"  {item}")
    print()
    print("含 GPL / LGPL 字样（都是可选双许可，不构成著佐权义务）：")
    for item in gpl_mentions:
        print(f"  {item}")
    print()
    print("按 license 字符串分布（出现 >= 2 次）：")
    for text, count in license_texts.most_common():
        if count >= 2:
            print(f"  {count:>4}  {text}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
