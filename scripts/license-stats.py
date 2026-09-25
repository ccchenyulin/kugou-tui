#!/usr/bin/env python3
"""统计依赖的许可分布，用来复核 `docs/LICENSES.md` 里那张表。

用法（在仓库根目录）：

    python3 scripts/license-stats.py

# 口径

* 数的是**全部**依赖，不区分目标平台（`Cargo.lock` 里有什么就数什么）。
* `license` 字段取自 `cargo metadata`。**不依赖本机是否解包过某个 crate**——
  早先这里直接读 `$CARGO_HOME/registry/src/<name>-<version>/Cargo.toml`，于是
  「读不到」的数量会随本机缓存漂移：同一份 `Cargo.lock`，一次跑出 26 个读不到、
  一次跑出 102 个，数字根本没法复核。换成 `cargo metadata` 后，结果只取决于
  `Cargo.lock`。
* 只做**分类统计**，不判断许可兼容性。真要严格审查请用 `cargo-deny` / `cargo about`。

依赖变动后重跑一次，把输出里的数字同步到 `docs/LICENSES.md`。
"""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys
from collections import Counter

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent

# 判定「弱著佐权」的许可标识。只有**纯** MPL-2.0 才算；
# `MIT OR MPL-2.0` 这类可选双许可按宽松算（整体按 MIT 用即可）。
COPYLEFT = "MPL-2.0"


def load_packages() -> list[dict]:
    """用 `cargo metadata` 取全部依赖。

    `--locked` 是为了**不改动 `Cargo.lock`**：这个脚本只读不写，
    万一 lock 与 `Cargo.toml` 不一致，宁可报错也不要它顺手改掉。
    """
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(
            "cargo metadata 失败，先确认在仓库根目录且 cargo 可用：\n"
            f"{result.stderr.strip()}",
            file=sys.stderr,
        )
        raise SystemExit(1)

    metadata = json.loads(result.stdout)
    members = set(metadata.get("workspace_members", []))
    # 本仓库自己不算第三方依赖
    return [pkg for pkg in metadata["packages"] if pkg["id"] not in members]


def main() -> int:
    packages = load_packages()

    permissive = 0
    copyleft: list[str] = []
    unreadable: list[str] = []
    gpl_mentions: list[str] = []
    license_texts: Counter[str] = Counter()

    for pkg in packages:
        name = pkg["name"]
        license_text = (pkg.get("license") or "").strip()

        if not license_text:
            unreadable.append(f"{name}-{pkg['version']}")
            continue

        license_texts[license_text] += 1

        if license_text == COPYLEFT:
            copyleft.append(name)
        else:
            permissive += 1

        if re.search(r"GPL", license_text, re.IGNORECASE):
            gpl_mentions.append(f"{name} ({license_text})")

    total = permissive + len(copyleft) + len(unreadable)
    print(f"依赖共 {total} 个（不含本仓库自己）")
    print(f"  宽松许可（含可选双许可）    {permissive}")
    print(f"  弱著佐权 {COPYLEFT}          {len(copyleft)}  -> {sorted(copyleft)}")
    print(f"清单里没有 license 字段     {len(unreadable)}")
    for item in unreadable:
        print(f"  {item}")
    print()
    print("含 GPL / LGPL 字样（若为可选双许可，则不构成著佐权义务）：")
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
