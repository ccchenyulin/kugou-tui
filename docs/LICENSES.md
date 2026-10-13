# Licenses

## This Project

This project is under **MIT** (© 2026 kugou-tui contributors).

The license text is in the repository root `LICENSE`; when installed via a package manager it is at `/usr/share/licenses/kugou-tui/LICENSE`.

> Deliberately no relative link to `LICENSE` here: this document gets installed to `/usr/share/doc/kugou-tui/docs/`, while the license sits under `/usr/share/licenses/` per Arch convention—a relative link would necessarily break after installation. A path reads correctly in both cases.

## Third-Party Dependency Licenses

`Cargo.lock` has **523 packages** in total (including this repository's two workspace crates: `kugou-tui` and `libresoda-fork`; not split by target platform). Excluding this repository leaves **521 third-party dependencies**, distributed as follows:

| License | Count | Notes |
|---|---|---|
| `MIT` / `Apache-2.0` and their various combination spellings (including BSD / ISC / Zlib / Unlicense / WTFPL / 0BSD / BSL-1.0 / CC0 / CDLA / LLVM-exception, and Unicode-related) | **508** | Permissive, pick either one |
| **`MPL-2.0`** | **13** | **Weak copyleft**: the symphonia series (FLAC / MP3 / Vorbis / AAC decoders) and `option-ext`. File-level copyleft; distributing statically linked binaries requires keeping their source available |
| No `license` field in the manifest | **0** | — |

How the stats are computed: `cargo metadata` (reads `license` from the registry index and manifests, **not dependent on whether a crate happens to be unpacked on this machine**). To reproduce, run once from the repository root:

```bash
python3 scripts/license-stats.py
```

> Earlier this script read `$CARGO_HOME/registry/src/<name>-<version>/Cargo.toml` directly, so the "unreadable" count drifted with the local cache—for the same `Cargo.lock`, one run gave 26 unreadable and another gave 102, making the numbers unreproducible (the old table in this file was skewed by exactly this pitfall). With `cargo metadata` the result depends only on `Cargo.lock`.

Points to note:

1. **MPL-2.0**: if distributing this project as a binary, ensure the source of the symphonia-related MPL code stays available (keeping `Cargo.lock` and the target platform's dependency-fetch method satisfies this). Also `termina` is `MIT OR MPL-2.0`, an **optional** dual license, usable as MIT overall, imposing no copyleft obligation.
2. **`ring` / `aws-lc-rs` / `aws-lc-sys`**: crypto libraries, pulled in with **non-Windows** `reqwest` + `rustls` (on Windows TLS goes through the system SChannel, see `Cargo.toml`). Their licenses are **conjunctive** combinations such as `Apache-2.0 AND ISC` and `ISC AND (Apache-2.0 OR ISC)`; `aws-lc-sys` additionally stacks BSD-3-Clause and MIT. These libraries may be subject to export controls in some jurisdictions; confirm before commercial distribution.
3. **`native-tls` / `openssl` / `openssl-sys` / `foreign-types` / `vcpkg`**: this table **does not distinguish target platforms**, so they are in the list too—they are the OpenSSL backend that `native-tls` declares for **non-Windows, non-macOS** targets. This project only uses `native-tls` under `cfg(windows)` (where it goes through `schannel`, never touching OpenSSL), and Linux builds do not compile these packages at all, so the actual artifact contains no OpenSSL code.
4. **The 3 packages mentioning GPL / LGPL** are all **optional** dual licenses, usable as permissive overall, imposing no copyleft obligation: `self_cell` (`Apache-2.0 OR GPL-2.0-only`, pulled in with ratatui-image), and two versions of `r-efi` (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`).

> This page does only **categorical statistics**, not license-compatibility judgments. For stricter checks use [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) or [`cargo-about`](https://github.com/EmbarkStudios/cargo-about).
