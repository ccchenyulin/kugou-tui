# Release and Cleanup

This document covers **version and tag rules**, **what a release artifact contains**, **the
order and dependencies of the release flow**, and **when each cleanup path runs and what it
clears**.

Implementation: `scripts/release` (release flow), `scripts/clean` (build artifact cleanup),
`src/audio/streaming.rs` + `src/audio/cache.rs` (runtime cleanup). Only **verified behavior**
is described; anything unverified is called out.

---

## 1. Version and tag rules

| Item | Rule | Enforced by |
|---|---|---|
| Version | `Cargo.toml` `version`, a plain numeric `X.Y.Z` (no `-alpha` suffix) | `scripts/release` pre-check |
| Tag | Always `vX.Y.Z`, **annotated** (`git tag -a`), release notes in the message | `scripts/release` |
| Tag uniqueness | Checked locally and remotely; abort if it exists | `scripts/release` |
| Changelog | `CHANGELOG.md` must have a `## [X.Y.Z]` section | `scripts/release` |
| AUR `pkgver` | Must equal the same version | `scripts/release` rewrites it |
| Binary self-reported version | The built binary's `--version` must match `Cargo.toml` | `scripts/release` |

**Tags are not redoable.** Once a version is used, pick a new one; never delete and re-tag —
AUR and crates.io versions are equally irreversible.

---

## 2. What a release artifact contains

The artifact is a tarball, `kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz` (5.3 MB
measured at 0.6.2), produced by `scripts/make-release-tarball`. It **must be self-contained**:
it is **the only path available to non-Arch users** — the AUR package ships the API service
directly, this tarball does not, so it must carry everything needed to set the service up.

| Content | Why it must be there |
|---|---|
| `kugou-tui` (binary, ~11.7 MiB, stripped) | The main program |
| `scripts/kugou-api` | Start/stop the API service |
| `scripts/kugou-api-install` | **Fetches and configures the service** — without it users cannot set up the service and the flow breaks |
| `scripts/kugou-tui` | Launcher (starts the service on demand) |
| `README.md`, `README.zh-CN.md`, `KEYBINDINGS.md`, `CHANGELOG.md`, `LICENSE` | Basic docs (English + Chinese README) |
| `docs/*.md` | Full documentation |

`scripts/release` checks this list item by item and aborts if any is missing.

> Releases at 0.3.7 and earlier contained **only the binary and README**, missing three
> scripts — non-Arch users could not run `kugou-tui-install-api` after extracting. Assembling
> the directory by hand misses things easily, so this step is now a script.

**Not included**: the API service itself (a separate project, fetched by the user per
`docs/INSTALL.md`). Build with `cargo build --release`, **never makepkg** — `makepkg` applies
`makepkg.conf`'s `-march=native`, and that binary will not run on another machine.

### Packages for three platforms come from CI

`scripts/release` runs on the dev machine and produces the `x86_64-unknown-linux-gnu`
tarball; the other two platforms **cannot be built there — a physical limit, not an
oversight**: the Windows `.exe` needs the MSVC toolchain and the macOS binary the Apple SDK.

So all three platforms go through `.github/workflows/release.yml`: on a `v*` tag push it
builds one each on `ubuntu-latest` / `windows-latest` / `macos-latest`, packages them, and
attaches them to the same Release. Releasing then works **without the dev machine on hand**;
artifacts depend on no specific machine.

| Platform | Artifact | Packaging script |
|---|---|---|
| Linux x86_64 | `kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz` | `make-release-tarball` (CI and local) |
| Windows x86_64 | `kugou-tui-<version>-x86_64-pc-windows-msvc.zip` | `build-windows.ps1` (CI) |
| macOS arm64 | `kugou-tui-<version>-aarch64-apple-darwin.tar.gz` | `make-release-tarball` (CI) |

> **Linux has two sources.** Local `scripts/release` still builds and uploads one (it also
> verifies asset contents); CI builds another, same name, overwritten by
> `upload-release-asset`'s `--clobber` — last one wins. So **the two sha256 values are not
> guaranteed to match**: the rustc minor version and the target machine's C toolchain both
> affect the `aws-lc-sys` build. A hash recorded during a local release not matching one
> downloaded from the Release later is normal, not a replaced file. To have a single source
> of hashes, drop the Linux packaging and upload from `scripts/release` and leave it entirely
> to CI — a release-flow change, do it separately.

> **Intel Mac is out of scope**: `macos-latest` is an arm64 machine, so its binary will not
> run on Intel. To cover it, cross-compile with `--target x86_64-apple-darwin` on the same
> runner — a different package name (`x86_64-apple-darwin`) and one extra job; not done now.

### The packaging script is verifiable before the tag is pushed

`release.yml` only runs after the tag is pushed, and **tags are irreversible** — if the
packaging script misses a file, you either ship a broken package or accept it (this project
hit that at 0.3.7). So the packaging script has two verification points before pushing the
tag, **both outside `release.yml`**:

1. **Stage one** of `scripts/release` — runs `make-release-tarball` locally, checks the
   content list item by item, then stops for confirmation;
2. `release.yml`'s own **dry-run** (below) — runs the three platforms' build, packaging, and
   artifact passing end to end, skipping only Release creation and asset attachment.
   Platform-specific branches (BSD tools on macOS, the PowerShell packaging script on
   Windows) can only be verified by actually running.

> `ci.yml` **does none of this**: its three columns only run `clippy + test` — no release
> build, no release packaging, no artifact passing. Intentional: release builds use
> `lto = "fat"` and `codegen-units = 1`, so running them for three platforms on every push
> costs several times the time while nobody uses the artifacts (anyone wanting a binary
> should go to the Releases page). Reason documented in the `ci.yml` header.

### dry-run: run the whole pipeline without pushing a tag

Changing `release.yml` cannot be verified by "pushing a tag to try it" — tags are
irreversible. So `workflow_dispatch` has an extra `dry_run` input: the whole pipeline runs
(three-platform build, packaging, artifact upload, retrieval, asset list check), **skipping
only the final Release creation and asset attachment**; not a byte is written remotely.

It checks out `main`, and version checking compares `TAG` against the binary's `--version`,
so the tag passed must match `main`'s current version:

```bash
gh workflow run release.yml --ref <branch> -f tag=v0.6.1 -f dry_run=true
```

### Backfilling assets (`workflow_dispatch`)

To backfill assets for an **already released** version, specify the tag manually — e.g. this
workflow did not exist when 0.4.2 was released:

```bash
gh workflow run release.yml -f tag=v0.4.2
```

The backfill path checks out `main` (`scripts/release-notes` is only updated on main; the
copy in an old tag may not yet know the version being backfilled). `publish` first checks
whether the Release exists: if so it skips creation and uploads assets directly (`--clobber`
overwrites same-named files); if not it creates one.

### Timing: the Release appears only after all three platforms are green

The three build jobs (`linux` / `windows` / `macos`) **only build and package**, each
uploading its package as an artifact, none touching the Release. The fourth job `publish`
`needs` those three, retrieves the artifacts, checks the list (2 tarballs + 1 zip), then
creates the Release and attaches all assets **in one shot**. Workflow-level permission is
`contents: read`; write permission is granted only to `publish`. The release page therefore
never shows a window where "the Release exists but assets have not arrived", and a build
failure means no Release is created at all.

> It used to be the opposite: the first job `prepare` created the Release, then the three
> platforms each added assets — so any platform's build failure left an already-published
> Release whose assets could never be completed.

Local `scripts/release` remains usable and does more (verifies the CHANGELOG has the version
section, updates and pushes AUR, downloads assets back to verify). Its order is "push tag →
create Release → upload assets", which collides with `publish`: both are idempotent —
`publish` skips creation if the Release exists, and `upload-release-asset` uses `--clobber` —
**whoever arrives first creates it, no errors either way**.

> **The cost of tag-only pushes**: AUR's `PKGBUILD` / `.SRCINFO` are not updated and
> `CHANGELOG.md` is not verified. To also release to AUR, run `scripts/release`, or manually
> update `~/aur/kugou-tui` afterward.

### Current state: AUR is not published, always pass `--no-aur`

The package `kugou-tui` **does not exist on AUR** yet
(`aur.archlinux.org/packages/kugou-tui` returns 404), and local `~/aur/kugou-tui` is a draft
directory with no `.git`. Therefore:

- The release command is fixed to `./scripts/release --no-aur`;
- Omitting `--no-aur` aborts in the pre-check (the script checks whether `$aur_dir/.git`
  exists). Intentional — otherwise the flow would first push main and tag
  (**irreversible**), then hit "not a Git repository" at the `git add` in step 9, paying the
  cost for nothing.

Publishing to AUR requires these external actions first (none in this repo):

1. Register an AUR account and paste `~/.ssh/aur.pub` into My Account → SSH Public Key;
2. `git clone ssh://aur@aur.archlinux.org/kugou-tui.git` (AUR creates the repo on first use);
3. Put the local `~/aur/kugou-tui`'s `PKGBUILD` / `.SRCINFO` / `LICENSE` /
   `kugou-api-package-lock.json` into that repo and push.

Afterward, delete this section and change `docs/INSTALL.md`'s "Path 4: AUR (planned, not yet
published)" to normal instructions.

The zip also carries three **bash** scripts (still useful under Git Bash / WSL), and the
macOS tarball likewise — a package carries "things also usable on another platform", not
"this platform's native scripts", which the source repo provides.

---

## 3. Target repositories for the release push

| Target | URL | What is pushed | Credentials |
|---|---|---|---|
| GitHub (source and Release) | `github.com/sijin-xb/kugou-tui` | `main` branch, `vX.Y.Z` tag, Release assets | `gh` logged in |
| AUR | `ssh://aur@aur.archlinux.org/kugou-tui.git` (local at `~/aur/kugou-tui`) | `PKGBUILD` and `.SRCINFO` on the `master` branch | Separate key `~/.ssh/aur`, public key posted to the AUR account |

AUR **only accepts the `master` branch**; `.SRCINFO` must be pushed too, or the page will not
update its version. The local repo's git identity is repo-level
`sijin-xb <sijin-xb@users.noreply.github.com>` — **the author email appears in cleartext in
AUR commit logs**, so use GitHub's noreply rather than a real email.

---

## 4. Execution order and dependencies

`./scripts/release` does it all in one command. Two stages; **stage two is entirely
irreversible external actions**, so the script stops after stage one, lists every action it
will take, and waits for confirmation.

### Stage one: local (repeatable, no external effect)

| # | Step | Depends on |
|---|---|---|
| 1 | Pre-check (version format, branch, clean worktree, in sync with origin, tag unused, CHANGELOG has the section) | — |
| 2 | Clean the previous run's intermediates (calls `scripts/clean`) | 1 |
| 3 | `cargo clippy --all-targets` + `cargo test` | 2 |
| 4 | `cargo build --release`, and verify the binary's self-reported version | 3 |
| 5 | Build the release package, and check the content list item by item | 4 |
| 6 | List stage-two actions, wait for confirmation | 5 |

### Stage two: external (irreversible)

| # | Step | Depends on | Notes |
|---|---|---|---|
| 7 | `git push origin main` | 6 | Network action, with backoff retries |
| 8 | `git tag -a vX.Y.Z` + `git push origin vX.Y.Z` | 7 | **Irreversible** |
| 9 | Take the sha256 of the tag source tarball, rewrite AUR's `pkgver` and `sha256sums`, regenerate `.SRCINFO`, commit locally (**no push**) | 8 | **The only dependency that cannot be reordered**, see below |
| 10 | Clone the AUR repo from that commit and run `makepkg` once | 9 | Verifies "what a user clones builds" |
| 11 | `gh release create` and attach the release packages | 8 | Idempotent: if it exists, switch to `gh release upload --clobber` |
| 12 | **Download** the Release assets back to verify content and sha256 | 11 | Cannot rely on local files alone |
| 13 | Push AUR (`master`) | 9 | If the key is not registered, only warn, no error |

### `--no-aur`: skip the three AUR steps

When the AUR account cannot be registered (hit in practice), `./scripts/release --no-aur`
runs the whole flow without touching AUR: steps **9, 10, 13** are skipped, and `PKGBUILD` and
`.SRCINFO` are not changed by a single byte.

The action list printed before stage two **changes accordingly** — the three AUR items do not
appear, numbering shifts, and it states "`--no-aur` in use". The point of that list is to let
a person confirm "what is about to happen" before pressing Enter; listing something and then
not doing it would be a lie.

⚠️ It is **not a rehearsal**: pushing main, tagging, and creating the Release all happen —
**it really releases**, it just does not release to AUR. To only check without releasing, use
`--dry-run`.

### Why step 9 must follow step 8

AUR's `sha256sums` needs the checksum of the **tag source tarball on GitHub**, and that
tarball is only generated by GitHub after the tag is pushed. So the AUR update **cannot** be
moved earlier into stage one — the only dependency in the flow that cannot be reordered; the
script has a dedicated comment for this.

(The alternative is to have AUR use a `git+...#tag=` source, which needs no checksum and
could be updated earlier; but versioned releases use tarball + checksum to match AUR
convention and to guard against upstream tampering, so it was not adopted.)

### Failure and recovery

- Any stage-one step fails: nothing was released; fix and rerun.
- Stage two fails midway: the script states exactly which step it stopped at. **Do not delete
  the tag and start over**; keep what was pushed and fill in the missing parts by hand (steps
  9–13 are all idempotent and can be rerun individually).
- A step-12 failure **must be stopped and investigated** — it means what was built locally
  differs from what users download.

---

## 5. Cleanup logic

### 5.1 Build and packaging artifacts (implemented: `scripts/clean`)

**When it triggers**

1. **Before** a release starts — called automatically by `scripts/release`, ensuring the
   build starts from a clean state
2. **After** a successful release — can be run again as well
3. **Manually** — anytime disk space is tight

**What it clears (default)**

| Clears | Location | Measured usage |
|---|---|---|
| ✅ | `target/package/` | 4.7 MiB |
| ✅ | tarballs in `dist/` for **versions other than the current one** | 3.2 MiB / version |
| ✅ | AUR repo's `src/`, `pkg/` | — |
| ✅ | upstream source copies fetched by makepkg in the AUR repo | — |
| ✅ | the `.pkg.tar.zst` from the previous build in the AUR repo | 5.6 MiB |

About **10 MiB** per run; keeping several versions means hundreds of MiB.

**Explicitly left alone** (clearing them needs an explicit flag)

| Flag | What it clears extra | Why not by default |
|---|---|---|
| `--all` | the whole `target/` | Destroys incremental compilation; the next full rebuild takes ~3 minutes. **But it is the biggest item here**: measured `target/` at **26 GiB** (`debug/` 25 GiB — clippy and test output; `release/` 1.2 GiB). When disk is tight this beats everything else combined |
| `--cache` | `~/.cache/kugou-tui` (including downloaded songs) | That is **user data**, not a build artifact |

`--dry-run` only reports, does not delete. Before every deletion it confirms "this really is
what we think it is" — if the AUR directory has no `PKGBUILD`, it skips outright, preferring
to delete nothing.

### 5.2 Runtime memory (implemented: `src/audio/streaming.rs`)

The streaming buffer **keeps only a tail window** (`MAX_KEPT_BYTES = 4 MiB`; past that it is
reclaimed to `TARGET_KEPT_BYTES = 1 MiB`), and bytes outside the window are read back from
the on-disk `.part` file with positioned reads (Unix `pread` / Windows `seek_read`). Resident
memory is therefore decoupled from track size: a 64 MiB Hi-Res file occupies only a few MiB,
and seeking back into a reclaimed region still plays (that part is on disk). Design and
measurements in [DESIGN.md](DESIGN.md#streaming-playback).

Trigger and watermarks: `push` checks "reclaim once only when the memory window exceeds
`max`", dropping to `target` in one go — not on every push, since reclaiming is a
`Vec::split_off` plus an allocation and frequent calls are costly. This "act only on
overflow, drop all the way to target" pattern matches the disk cache's `EVICT_TARGET_RATIO`
(drop to 80%).

Only bytes **before the read position** are dropped; bytes after it never are (the decoder is
about to read them). The look-back window is defined by "how far behind the read position",
independent of the clock, so there is no timed reclamation.

Guard test: `streaming::tests::keeps_only_a_bounded_window_in_memory` (asserts window length
rather than RSS — unit tests share a process with parallel tests and the allocator cache, so
`VmRSS` cannot be asserted; see [MAINTENANCE.md](MAINTENANCE.md) §5). RSS-level comparison
experiments are done out of process; method and baseline in
[MAINTENANCE.md](MAINTENANCE.md) §6.
