# Contributing

How to get the project running, the code conventions, and the checks to run before committing.

## Getting the environment running

1. **Rust 1.90+** (edition 2024; the floor is pushed up by a **dependency** — `quantette` needs 1.90).
2. **Node.js 12+ (optional)**: only needed when debugging the `--api node` fallback or running the NetEase
   source. The **default `--api native` implements the Kugou API in-process and needs no Node.js** — it
   suffices for everyday development, and you can browse and play without logging in. For the fallback path,
   the API service [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi) is a separate repo (no submodule,
   no vendor), fetched separately. Easiest:

   ```bash
   ./scripts/kugou-api-install kugou   # clone + npm install + start two instances
   ```

   Doing it by hand is fine too, but **pin to a verified commit** (upstream is an active repo and API fields
   drift):

   ```bash
   git clone https://github.com/MakcRe/KuGouMusicApi.git && cd KuGouMusicApi
   git checkout a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e
   npm install && npm start            # note: npm start, not npm run dev
   ```

3. Build and run:

   ```bash
   cargo build --release && ./target/release/kugou-tui
   ```

You can browse and play without logging in; QR scanning is not needed for general development.

## Run these three before committing

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

**clippy runs with `-D warnings`**: any warning fails the build. CI
(`.github/workflows/ci.yml`) runs clippy + test on each of Linux / Windows / macOS
(`cargo fmt --check` and the no-default-features combination only on the Linux column —
neither is platform-dependent). The Windows and macOS columns are not decoration: path
expansion, config directories, cache file naming, and audio backends only surface when
actually run, and the dev machine is usually Linux.

**CI only verifies, it produces no binaries** — none of the three columns runs
`cargo build --release`, packages a release, or passes artifacts. To get a runnable binary:
the Releases page (produced by `release.yml` on tag push) or a local `cargo build --release`.
The reason is in the `ci.yml` header.

When touching platform-specific code, additionally confirm the Windows target builds:

```bash
rustup target add x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc --all-targets
```

> The MSRV is **1.90**, in `Cargo.toml`'s `rust-version`, pushed up by a **dependency**, and
> it drifts with `cargo update`. To confirm the floor after changing dependencies:
> `cargo +1.90.0 check --locked --all-targets`.

## Code structure

```
src/
├── main.rs          Entry: allocator policy, argument parsing, logging, API service bootstrap, start App
├── bootstrap.rs     Auto-bootstrap of the local API service (KuGouMusicApi)
├── bootstrap_disabled.rs  Same-named stand-in when the node-bootstrap feature is off (all errors)
├── cli.rs           clap command-line arguments
├── config.rs        TOML config persistence (with defaults and valid-value constants)
├── error.rs         Unified error type AppError
├── logger.rs        Minimal file logging (no tracing) + in-memory tracing
├── event.rs         Event bus and the Loaded enum
├── keymap.rs        Key → Action mapping, plus the CHEATSHEET for the help panel
├── mpris.rs         MPRIS (desktop media controls), Unix only
├── tray.rs          System tray (StatusNotifierItem), Unix only
├── window.rs        niri window control (minimize / show)
├── util.rs          Time formatting, random numbers, small utilities
├── api/
│   ├── traits.rs    Unified source interface
│   ├── client.rs    HTTP client: auth params, error-code parsing, response-cache bypass
│   ├── model.rs     JSON → domain model parsing (all multi-candidate key names live here)
│   ├── catalog.rs   Browse APIs: search / playlist / artist / charts
│   ├── cloud.rs     Login and cloud playlist write operations
│   ├── lyric.rs     Lyric fetching and KRC/LRC decoding
│   ├── node.rs      `--api node` fallback: implementation over the local KuGouMusicApi
│   └── native/      Embedded pure-Rust Kugou API (default backend)
│       ├── mod.rs       26 routes implemented in-process
│       ├── sign.rs      Two signature schemes (Android / Web)
│       ├── crypto.rs    RSA / AES / MD5
│       ├── device.rs    Device fingerprint
│       ├── krc.rs       KRC lyric decryption
│       └── transport.rs Outbound request construction
├── app/
│   ├── mod.rs       App assembly, main loop, input thread
│   ├── state.rs     AppState and the list structures
│   ├── update.rs    Event → state change (the only place state is changed)
│   ├── playback.rs  Playback control (start / skip / progress / volume)
│   ├── cloud.rs     Login, cloud playlists, and source switching
│   ├── desktop.rs   Push state to MPRIS and the tray
│   ├── navigation.rs Tabs and focus
│   ├── queue.rs     Play queue and play modes
│   ├── search.rs    Search
│   ├── session.rs   Session persistence
│   └── settings.rs  Settings page
├── audio/
│   ├── engine.rs    rodio playback engine (separate thread + atomics)
│   ├── streaming.rs Streaming in-memory window and spill to disk
│   ├── download.rs  Audio download (write .part then atomic rename)
│   ├── cache.rs     Cache directory management and LRU eviction
│   ├── resample.rs  Anti-aliasing resample of hi-res masters
│   ├── downmix.rs   Multichannel downmix
│   ├── levels.rs    Level capture and spectrum computation
│   └── spectrum.rs  FFT
├── source/          Source abstraction (Kugou / Concept edition / NetEase / Soda)
│   ├── netease.rs   NetEase (over the local NeteaseCloudMusicApi)
│   └── sodam/       Soda (direct public network + CDP signing page)
└── ui/
    ├── mod.rs       Layout
    ├── theme.rs     Colors (true color / 16-color sets)
    ├── icons.rs     Icons (Nerd Font / ASCII sets)
    ├── widgets.rs   Rendering primitives: list rows, panels, QR codes, etc.
    └── views/       Sidebar, list, play bar, lyrics, help, settings, sources, visualization

crates/
└── libresoda/       Soda Music data plane (separate crate, package name libresoda-fork)
```

## Conventions

### Comments explain "why", not "what"

The code already says what it does; comments add **context you cannot see by reading the
code**: why a field has two names, why a timestamp is added somewhere, where a magic number
came from. Each module's top `//!` doc explains that module's design trade-offs.

### UI uses ASCII markers only

Symbols in the interface (selection marker `>`, now-playing `*`) are always ASCII. Fancy
Unicode symbols become tofu boxes in terminals lacking the font, or throw off column-width
math.

### Truncate text by display width

Chinese characters and full-width punctuation occupy 2 columns. All truncation must go
through `display_width` / `truncate_to_width`; slicing with `chars().count()` misaligns mixed
Chinese/English rows.

### New dependencies need justification

One of this project's goals is "lightweight" (release binary ~**11.7 MiB**, resident
~**19 MiB**; measurements in [docs/DESIGN.md](docs/DESIGN.md#low-resource-footprint)). Before adding a
dependency, state: what problem it solves, whether a few dozen lines could replace it, and
how large a dependency tree it brings. `tracing` / `chrono` / `rand` are currently
deliberately excluded.

### Parse upstream responses leniently

KuGouMusicApi is a reverse-engineered wrapper: **response structures drift and the docs
disagree with reality**. Conventions:

- Try the same semantic across a **list of candidate key names** in order (`pick_string` /
  `pick_i64`, etc.).
- Do not assume types: `AlbumID` may be a number, a string, or `null`.
- A single failed parse drops only that item, not the whole page.
- When adding parsing logic, note doc inconsistencies in the "upstream docs say / actually
  is" table in [docs/DESIGN.md](docs/DESIGN.md#interface-adaptation).
- **The same upstream may have several field layouts.** NetEase is the example: `/search`
  gives `artists` / `album` / `duration`, while `/playlist/track/all` and `/artists` give
  `ar` / `al` / `dt` — recognizing only the former leaves the search page fine but every song
  on the playlist and artist pages with no artist, no album, and a `00:00` duration. **Every
  layout must be pinned with a unit test.**

### State is only changed on the main thread

`app/update.rs` is the only place that modifies `AppState`; the audio thread shares playback
position, duration, and volume only through atomics. This avoids locking and makes state
changes traceable.

## Adding a new API

1. Add an `async fn` in `api/catalog.rs` (or `cloud.rs` for write operations), using
   `self.get_json_uncached(...)` or `get_json(...)`.
2. Add the corresponding parsing function in `api/model.rs`, following the lenient
   conventions above.
3. Add a variant to the `Loaded` enum in `event.rs`.
4. Add a fetch method + `Loaded` branch handling in `app/update.rs`.
5. If a new key is needed: add an `Action` variant, the key mapping, and the `CHEATSHEET`
   entry in `keymap.rs`, plus the action name in `REBINDABLE` (users rebind by this name in
   `[keymap]`); finally sync the action-name list in
   [docs/CONFIGURATION.md](docs/CONFIGURATION.md#custom-keybindings). **A missing action name makes
   the test tell you on the spot** — `every_cheatsheet_shortcut_can_be_rebound_by_name`
   compares the two lists. (The CONFIGURATION.md list can only be synced by hand; tests do
   not cover it.)
6. If it needs display: add list fields in `app/state.rs` and rendering in `ui/`.
7. Add unit tests — especially for parsing.

**The Kugou API has two backends.** The default `--api native` (`api/native/`) implements
routes in-process; `--api node` (`api/node.rs`) goes through the local KuGouMusicApi;
`api/traits.rs` is the shared interface and `app/` only knows that. When adding or changing a
Kugou API, both sides must change, and:

- native-side signing/encryption/device-fingerprint parts, which **fail silently when
  wrong**, need a KAT (live baseline values under `tools/kat/`; usage in
  [tools/kat/README.md](tools/kat/README.md));
- when field parsing disagrees, record the "upstream docs say / actually is" difference in
  [docs/DESIGN.md](docs/DESIGN.md#interface-adaptation).

## Verification

**Do not verify from docs alone.** Almost every pitfall this project has hit came from "the
docs say this, reality does otherwise". When a change touches the API:

- Hit the real service with `curl` and save the raw responses for comparison;
- Start the app and click through it, confirming you can "see concrete content" (not just
  "the list loaded").

When debugging key issues, turn on verbose logging (it records what each key maps to):

```bash
KUGOU_TUI_DEBUG=1 ./target/release/kugou-tui
```

The log path is shown by `kugou-tui --print-config`.

## Commits and PRs

- A commit message states in one line what changed; split multiple changes into multiple
  commits.
- In a PR, state: **what changed, why, and how it was verified** (for API changes, a short
  snippet of the key fields from a real response helps a lot).
- When a change affects user-visible behavior, update `README.md` accordingly.

## Release

### 1. Bump the version and changelog first

```bash
# Cargo.toml version (plain numeric X.Y.Z; Cargo.lock changes too, commit it together)
# Add a section at the top of CHANGELOG.md, format per [Keep a Changelog]
git add -A && git commit -m "chore: version X.Y.Z" && git push
```

### 2. Publish to crates.io

```bash
cargo publish          # requires a crates.io account with a verified email
```

`scripts/release` **does not do this step**. crates.io renders the **README from that
version's package** — a changed README without a new release leaves the page unchanged.

### 3. Tag and create the Release

```bash
./scripts/release              # stage one local build, stage two push after confirmation
./scripts/release --dry-run    # check only, build and push nothing
./scripts/release --no-aur     # skip AUR (AUR push uses SSH and always fails when the key is unset)
```

It automatically does: pre-check → clean the previous run's intermediates → clippy + test →
build → package and check the content list → **stop and wait for confirmation** → push main →
tag → push tag → update and verify AUR → create Release → download back to verify → push AUR.

> **AUR is not published; always pass `--no-aur` for now**: omitting it aborts in the
> pre-check (the script checks whether `$aur_dir/.git` exists) and neither `PKGBUILD` nor
> `.SRCINFO` is changed by a single byte. External actions needed to publish are in
> [docs/RELEASE.md](docs/RELEASE.md#current-state-aur-is-not-published-always-pass---no-aur).

**Full spec (version and tag rules, artifact list, per-step dependencies, recovery after
failure) in [docs/RELEASE.md](docs/RELEASE.md).**

### Two most easily forgotten

1. **Tool scripts must be committed before tagging.** `scripts/make-release-tarball` was once
   committed after the tag, so checking out that tag did not get it. `scripts/release`'s
   pre-check requires a clean worktree, which blocks this mechanically.
2. **The upstream commit is pinned in two places**: `PINNED[kugou]` in
   `scripts/kugou-api-install`, and `_api_pin` in AUR's `PKGBUILD`. Changing `_api_pin` also
   requires regenerating `kugou-api-package-lock.json` in the AUR repo (that upstream commit
   only has `pnpm-lock.yaml`, while the build uses `npm ci`). The generation command is in a
   comment in the PKGBUILD.

### Disk

One release leaves about 10 MiB of intermediates in `target/package/`, `dist/`, and the AUR
repo's `src/` and `pkg/`. `scripts/release` reclaims them before starting; you can also run
it manually:

```bash
./scripts/clean --dry-run   # first see what it wants to delete
./scripts/clean             # reclaim
./scripts/clean --all       # also clear target/ (next full rebuild takes ~3 minutes)
```

## Legal and compliance

This project is a personal learning/research tool: the embedded backend (`--api native`)
ports the Kugou API's signing and decryption item by item from the open-source project
[KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi), and the `--api node` fallback calls
that service. When contributing code, ensure:

- no implementation that cracks, bypasses authentication, or circumvents copyright
  protection (the ported signing/decryption is only for accessing Kugou's official service
  with the user's own account, and must not be used to circumvent membership or copyright
  restrictions);
- no code that uploads user data (this project has no telemetry; login credentials exist only
  locally);
- changes involving third-party services state their purpose and compliance impact in the PR.
