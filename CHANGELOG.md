# Changelog

All notable changes to this project are recorded here.
Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioning follows [Semantic Versioning](https://semver.org/).

## [0.6.2] - 2026-10-11

### Changed

- **Terminal layout rework**: the sidebar drops the icon in front of each item, and navigation becomes a compact "pointer / shortcut / title" layout; removed the `kugou-tui N/N` line that duplicated the title, the "playback" status block that duplicated the playback bar, and the level meter under the navigation; unified panel spacing; when the sidebar is too short it collapses to a single condensed status line (e.g. `● embedded · kugou`) instead of leaving a `… N more …` placeholder.
- **Visualizer page reworked**: eighth-cell-precision bar tops, in-bar gradients, an independent peak marker, a ground line, and a centered track-info row; idle and buffering states each have a defined layout.
- **Playback bar**: the right side shows segments by priority (loop mode > volume > next), so loop mode no longer disappears on narrow terminals.

### Fixed

- **Wrong lyrics fetched**: validate the duration and artist/title of lyric candidates and reject candidates that are "a different song"; when no matching lyric is found, show "instrumental, enjoy" instead of another song's lyrics. Also filter out credits lists (lyricist/composer/arranger…) and "Title - Artist" header lines that were being rendered as lyrics.
- On narrow terminals the title/artist/album columns of the track list shrink to available width, dropping whole columns when necessary, so the duration is no longer squeezed into `03:2`.

## [0.6.1] - 2026-10-09

### Fixed

- The login dialog no longer crashes on terminals shorter than 7 rows (`clamp(7, area.height)` panics when `min > max`).
- No more startup deadlock when log redirection fails (re-locking while holding the `SINK` lock).
- No more slice panic when the VIP expiry date hits a non-ASCII `end_time`.
- The system tray now detects a broken D-Bus connection and reconnects instead of silently freezing when signal sends fail repeatedly.
- When a streaming download task terminates abnormally it finalizes the write side and deregisters, so readers no longer block forever.
- After removing a track from the playback queue the highlight no longer shifts or goes out of bounds.
- The NetEase service uses its own instance name (`api-netease.pid`) and no longer contends for a PID file with Kugou standard edition.
- Several silently swallowed errors now log (KRC decode, Soda cover/membership queries), and a `clienttime` parse failure is now an explicit error.
- Reading a PID file tolerates a UTF-8 BOM (PowerShell 5.1's `Set-Content` may add one, which otherwise breaks parsing); handled on both the Rust and `kugou-api` (bash) sides.
- Audio cache keys now include the source (`<source>-<hash>-<quality>`), so tracks from different sources no longer share one cache file; old plain `hash-quality` cache files no longer match and are re-downloaded once.
- Stream URL resolution failures (cannot reach the target source, network interrupted) no longer get stuck on "buffering": the loading state is cleared and Space works again (previously playback stayed at `Loading` forever and even repeated Space presses did nothing).

### Changed

- Service log filenames are unified as `api-<name>.log` (launcher and `kugou-api` agree).
- The NetEase port `NETEASE_PORT` can now be written to the config file (like the other ports).
- The Windows cross-compile platform gate is tightened to `all(target_os = "linux", target_env = "gnu")`, no longer mis-affecting `*-pc-windows-gnu`.
- On Windows, an npm install timeout now kills the whole process tree (`taskkill /T /F`).
- When the target source is unavailable (unreachable, not logged in, service not started), the player automatically skips to the next playable track in the queue: within one round it skips sources already confirmed failed, skips at most `min(queue length, 5)` times, and does not skip under repeat-one; a manual track change clears the counter, and when all fail it stops in the stopped state with a single summary message. Per-track failures (region-locked, no VIP preview permission, etc.) still only report an error and do not skip.
- Cloud playlist add/remove/sync accept only tracks from the same source as the playlist: cross-source operations are refused with a message, and foreign-source tracks are no longer fed to the wrong client (bulk sync skips per track and states the skip count in the message).

### Performance

- The track list and playback queue build only the rows in the visible window each frame.
- Desktop integration (WebSocket / MPRIS / tray) compares before cloning each tick, no longer deep-copying the current track and lyrics unconditionally.
- The login QR code reuses already-rendered rows instead of re-encoding every frame (measured: ~0.9 ms saved per frame).
- Item lists (playlist plaza, artist list, etc.) build only the rows in the visible window each frame (2000 items measured: 577 µs → 251 µs per frame).

## [0.6.0] - 2026-10-09

### Added

- **Embedded pure-Rust interface layer.** Previously search, stream resolution, lyrics, and login were provided by a local Node.js service (KuGouMusicApi) and this program only launched it; now the 26 routes the client actually calls are implemented in-process, **no Node.js required**. The parts where "a mistake is silent" — signing, device fingerprint, KRC decryption — are ported function by function from upstream, each with its source file and function name in a comment, and verified byte-for-byte against the upstream JS on the same fixed inputs (KAT).

  This removes the install barrier: `cargo install` only places one binary in `~/.cargo/bin`, and previously users still had to prepare a Node runtime, otherwise "installed but unusable".

- **Embedded WebSocket service.** Pushes `lyrics` (current lyrics and playback position) and `playerState` (playback state), and accepts `control` commands (`toggle` / `next` / `prev`). On by default, listening only on `127.0.0.1:6520` — **no non-local connections accepted** — and it validates `Origin` and limits per-message size. `--no-ws` disables it; `--ws-port` changes the port.

- **`node-bootstrap` feature** (on by default). When off, the bootstrap code (port probing, downloading KuGouMusicApi, `npm install`, spawning `node app.js`) is excluded from compilation, so a binary built with `cargo build --release --no-default-features` does not contain it and is about 0.06 MiB smaller. `--api node` then fails with an explicit "this feature was disabled at build time" error instead of an ambiguous connection failure.

### Changed

- **The `--api` default changed from `node` to `native`.** Startup no longer launches a Node service and `ps` no longer shows an extra `node app.js` process. The Node backend is **kept** as a fallback and can be switched back with `--api node` (env var `KUGOU_API_BACKEND`) for comparison and troubleshooting.

- **WebSocket is on by default.** With no configuration at all, an extra listener appears on local `127.0.0.1:6520`. This is intentional, but it is a behavior change: disable it with `--no-ws` if unwanted.

- **`--api-start` now prints a notice and exits 0 under native.** Native has no such thing as a "resident service"; it used to print a line to stdout, but that `interface service ready: <address>` line on stdout is what scripts scrape the address from — a similar-looking format without an address means scripts parsing it get garbage. It now goes to stderr and the exit code is still 0 (this is not an error, just nothing to do).

### Fixed

- **Creating a cloud playlist no longer always reports failure.** The server puts the new playlist's `listid` in `data.info.listid`, while the parser only scanned the top level of `data`, so "the playlist was created but the program says it wasn't". Both the native and Node paths had this problem; fixed together.

- **Credentials in logs are always redacted.** `token` / `userid` / `dfid` / `mid` are replaced with `<redacted>` before being logged, covering three forms that previously leaked: the full URL of an outbound request, the Display of a `reqwest::Error` (which splices the URL in), and the first few characters of a non-JSON response body.

- **Removed the unused `ctr` dependency from the main package.** Its only use is `crates/libresoda`'s `src/soda/crypto.rs`, and that crate already declares `ctr` — the line in the main package was left over from when the Soda implementation was moved out.

## [0.5.0] - 2026-10-03

### Added

- **Soda Music source**, with its data plane provided by
  [libresoda](https://github.com/sodahub-org/libresoda) — the same library the official Soda client
  [SodaM](https://github.com/sodahub-org/sodam) uses. Login, search, stream resolution, decryption, and lyrics are all delegated to it, so behavior matches the official client and upstream fixes carry over. Adds the git dependency `libresoda` (not published to crates.io, pinned by `rev`).

  Two fundamental differences from the other three sources:

  - **Direct public-network connection** (`api.qishui.com`); no local service. `bootstrap` prepares nothing for it, first use needs no `--api-start`, and no Node.js;
  - **The delivered audio is encrypted** (MP4/CENC). Resolving a stream must complete "download the whole track → decrypt → write to disk" before handing it to the player, so Soda tracks **cannot be played while downloading**; first play waits for one full download. This is dictated by the encryption scheme, not an implementation shortcut.

- **VIP full tracks and lossless work out of the box.** Soda's stream resolution has two layers: the Web endpoint returns only 30/60 s previews, and the App endpoint requires an app-level signature to return the full track, and **without a signature it returns "HTTP 200 + empty body"** — which looks a lot like "membership not active" but is actually a missing signature. The upstream SodaM built-in public signing service is enabled by default, so no packet capture or self-hosting is needed; you can also use `[sources.sodam_app].signer_url` to switch to your own, or set it to `none` to disable explicitly (leaving only previews and free tracks).

- **QR login** (press `L` and select Soda). It uses libresoda's built-in **CDP signing page** and drives the local Chromium directly, **no Node** — which is also why it must go through a browser: the passport interface needs `a_bogus`, and the confirmed login state exists only in that browser session's cookie jar. On success you get a **session cookie**, written to `[sources.sodam].cookie`.

  The device fingerprint **may be left empty** (when going through the signing service, the signer has its own device identity), and manually filling in a cookie still works.

- **Downgrade reported honestly**: stream resolution picks by "completeness first, then quality" — if you want lossless but only a 128k full track is available, it picks the full track and states "actual quality standard" in the status bar; preview-only tracks explicitly state whether membership is the reason. There is no case where the UI shows flac while 128k plays with no way to find out why.

- **CLI**: `--sodam-cookie` / `--sodam-device-id` / `--sodam-iid` /
  `--sodam-signer-url` / `--sodam-signer-token` / `--sodam-x-helios` /
  `--sodam-x-medusa`, each with `SODAM_*` / `QISHUI_SIGNER_*` environment variables.
  Per-item override rather than whole-block replacement (so temporarily adding one item doesn't wipe the configured ones); empty and whitespace-only strings are treated as "not passed".

- **The downloader supports `file://`**: Soda's decrypted output is copied locally into the audio cache, so cache lookup, capacity reclamation, and next-track prefetch all work as usual, with no separate cache logic for Soda.

### Changed

- `SourceKind` gains `Sodam`; `SourceSet` gains the `sodam` and `sodam_app` fields. Old configs without these sections fall back to `Default`, and `normalize()` fills in the public-network address and **sets it enabled**.
- The source management page labels public-network sources as "direct public connection", so users don't think a local service should be running.
- The launcher scripts (both bash and PowerShell) gain a `sodam` branch and introduce `NEEDS_SERVICE`: when Soda is selected, the whole probe/launch/wait-ready flow is skipped. Previously it fell into the fallback branch and tried to launch KuGouMusicApi with "not running, starting… (port 3000)".
- **Cloud playlists, playlist detail, user profile, and membership status are all wired up.** Previously they returned "unsupported" for Soda, so the symptom was "logged in but nothing works". The real cause was that the dispatch layer wasn't wired, not that Soda lacks these interfaces. They now go through libresoda's public implementations (get_user_playlists / get_playlist_songs / fetch_pc_me / is_vip_account), and Capability's catalog / cloud / vip are all set to true.

  **Three features genuinely have no corresponding endpoint and keep explicit errors** (verified against the libresoda code, not omissions): playlist plaza (get_recommended_playlists and get_playlist_categories are `Unsupported` upstream, only playlist search works), artist list (only lookup by id, no hot-artists list), and rankings (no equivalent endpoint). The error text states the reason and the alternative.

### Fixed

- **Saving the config no longer overwrites Soda's login credentials.** `Config::save()`'s first action is to call `sync_active_source()`, which overwrites the source profile's cookie with the top-level `cookie` (Kugou's territory, always `None` for Soda) — so "login succeeds → save immediately → overwritten", showing up as **logged in, but after a restart it says not logged in**, and the cookie key is nowhere in the config file. Soda now does not take part in the mirroring; the old Kugou/NetEase paths are unchanged, and both sides have pinning tests.

- **Querying membership/profile right after login no longer fails.** `apply_server_cookie` used to write to `self.client_for(Sodam)` and then `set_cookie`, but when the source is already active, `client_for` returns `self.api.clone()` — modifying the copy has no effect on `self.api`, so the following request still used a client without the cookie and the server replied `1000016 login state expired`.

- **The QR code is now pure black and white.** The style used ANSI palette entries 0/15 (`Color::Black` / `Color::White`), and **terminals can remap those** (kitty allows changing ANSI 0-15), so "black modules" could be drawn gray, making the QR code faint and unscannable — looking like a terminal or font problem when it was actually fixable. Changed to hardcoded pure-black and pure-white RGB. The QR code has only two colors, and those two must be pure black and white; not following the theme is intentional.

- **QR code shrunk.** The module count is determined only by "content length + error-correction level", and the content (the scan address issued by the server) can't be changed, so lowering the error-correction level is the only lever: M (~15%) → L (~7%); screen display is the ideal condition, so L suffices; the quiet zone is also reduced from 2 modules to 1. Measured: Soda (193-character address) drops from about 59×30 to 51×26. Soda's address is 5× longer than Kugou's, so it was always bigger than Kugou's — not a rendering problem.

- **QR code adapts to terminal size.** Soda's scan address is much longer than Kugou's, producing a higher QR version, and the dialog size is computed from the QR code — so it filled the whole terminal. A QR code can't be scaled (its row count is determined by content length; denser would just distort the aspect ratio and make it unscannable), so it now checks whether it fits: if not, it draws nothing and shows a message stating "about N columns × M rows needed", with two ways out (enlarge the terminal and retry, or fill in the cookie by hand). Drawing a clipped QR code is worse than none — that code can't be scanned and the user just retries.

- `ApiClient` gains a `cookie()` accessor. The source modules need it to rebuild their own client with the login state — "what identity credentials exist" is general information and shouldn't be hidden by the HTTP layer.

## [0.4.8] - 2026-10-02

### Docs

This round **only touches docs**; the code is identical to 0.4.7. A separate release is needed because the crates.io page renders **the README in the package** — change the docs without a release and the page doesn't change.

- **All docs rewritten around "who is this for"**, totaling 1844 → 1473 lines:

  - README 192 → 162: removed the implementation arguments that had been stuffed in (why Windows doesn't use CMake / NASM, the 4 MiB window of streaming playback, the memory measurement method); the install section now leads with cargo;
  - INSTALL 590 → 425: removed the fish workflow that duplicated built-in features (probe / launch / teardown are now done by the program itself, all that's missing is a `git pull`), and compressed the explanatory passages in the Windows and macOS sections;
  - CONFIGURATION 318 → 256: the expanded "why" under each config item is compressed to one sentence, with the argument left to DESIGN.md;
  - USER_GUIDE 335 → 230: merged the duplicated descriptions in the interface layout and desktop integration sections;
  - FAQ: added four real questions introduced by 0.4.7 — node not found, why the first start waits half a minute, the service won't start / port taken, and how to investigate memory that only grows;
  - CONTRIBUTING: completed the code structure tree (`bootstrap.rs` / `mpris.rs` / `tray.rs` / `window.rs` / `source/` etc.), and added the crates.io step to the release section (`scripts/release` does not include it);
  - `DESIGN.md` and `MAINTENANCE.md` are **deliberately untouched**: they are the archive of "why", and their detail is the value.

- The facts in the docs got a round of machine verification: 43 internal links and anchors, 14 `kugou-tui --xxx` usages (aligned with `cli.rs`'s 17 long flags), and all `KUGOU_*` environment variables and config key names match the code.

## [0.4.7] - 2026-10-02

### Added

- **Automatic bootstrap of the interface service** (new `src/bootstrap.rs`). The first start prepares and launches KuGouMusicApi itself; the user only types `kugou-tui` throughout — this is also the prerequisite for publishing to crates.io.

  Why it's needed: `cargo install` only places one binary in `~/.cargo/bin`, carrying neither that Node.js service nor a usable post-install hook. Previously "installed but unusable": users had to clone the repo, install dependencies, and start two instances per platform themselves.

  - **Probe first**: if a service is already on the port, **reuse it and leave it alone on exit** — it may be one the user started, or one another kugou-tui instance started;
  - Only when the service is missing does it act, searching "configured `api_dir` → `/usr/share/kugou-tui/api/kugou` (distro package) → `~/.local/share/kugou-tui/api/kugou` → `~/KuGouMusicApi`", and only if all are absent does it download the pinned commit (`a5a9801`) and `npm install --omit=dev`;
  - Launches one instance only for the **current source**; when switching to another platform it starts that one too (spawn only, no install — the UI is already drawn and being frozen by npm for tens of seconds is unacceptable);
  - **Exit cleanup**: an instance it started stops on exit, leaving no resident node. `--api-start` is the explicit exception that requests residency, stopped with `--api-stop`;
  - Installation is file-locked, so two instances first-starting at once don't write into the same `node_modules`.

  **The prerequisite is unchanged: Node.js (>= 12) must be on the machine.** Without node the program says so clearly and exits — the Rust side can't conjure a Node runtime.

- Config items `api_auto_start` (default `true`) and `api_dir`; command line `--no-api-start` / `--api-start` / `--api-stop`; environment variables `KUGOU_API_AUTO_START` / `KUGOU_API_DIR`.
- The crate is published to [crates.io](https://crates.io/crates/kugou-tui), so `cargo install kugou-tui` works.

### Fixed

- **"Press Space or Enter to retry" was an empty promise** (the `StreamInterrupted` branch in `app/update.rs`). After the second stream interruption during streaming playback it just printed a notice and returned, and no key could catch it:

  - `Space` went through `toggle_playback()`'s normal start path, taking the position from `state.resume` (used for session restore, `None` here), so it **played from the start**, not from the break point;
  - `Enter` went through `activate()`, dispatched by `(Tab, Focus)` into "play the selected track / open a playlist", unrelated to the interrupted track;
  - And `start_playback` only cleared `stream_retried` when the hash was **different**, so after manually replaying the same track the allowance wasn't restored, and another interruption had no automatic fallback at all.

  Now, after an interruption the break point is stored in `pending_stream_retry`, pressing `Space` resumes from there and restores the automatic fallback allowance, and the notice becomes "press Space to retry from the break point" — `Enter`'s semantics are "enter the next level / play the selected item", kept predictable, no longer written into the notice. Changing tracks and stopping intentionally both invalidate this break point.

- **Streaming playback did not cancel the download task on "natural finish"** (`TrackFinished` in `app/update.rs`). It only wrote `active_stream = None` — releasing only the `Arc` in our hand, while the background download task held another copy and kept pouring the whole track into the memory window and the `.part` file. Track change and intentional stop both called `cancel()`; only the most common path missed it. Also moved the "return free pages" `malloc_trim` here: it previously ran only on the next track's `load`, by which point the new allocations had already happened and the just-returned pages were taken back.

- **The root cause of "memory only grows the longer you listen": the mmap threshold was set too high** (`src/main.rs`). It was pinned at 1 MiB, while Kugou covers decode to 300–900 KB bitmaps, **falling right below it**, so they went through the brk arena — `free` only returns pages to the arena, RSS stays at the peak, and it may only be reclaimed at the next `malloc_trim`, which is the source of "no leak but can't reclaim". Changed to 256 KiB so these blocks go through mmap directly and `free` becomes `munmap`, unconditionally returning to the kernel, no longer dependent on trim timing. Allocations below the threshold are unchanged.

  The basis is the comparison data from `audio::engine::tests::mmap_threshold_effect_probe` (allocation sizes vary by round, neither segment trims): at 1 MiB, 20 rounds rise +192 → +332 KiB monotonically then stop; at 256 KiB it is a constant +0. **A fixed size can't measure this** — it can always reuse the same batch of chunks.

- **MPRIS pushes were unstable** (`src/mpris.rs`, `src/app/desktop.rs`). Three independent causes, none as simple as "the signal wasn't sent":

  1. **`mpris:trackid` was a fixed path** (`/org/kugou_tui/Track/1`). The MPRIS spec defines it as "the track's unique identity", and clients (DMS / Quickshell / various status bars) use it to decide "did the track change". Being fixed meant that even when the metadata clearly changed, a spec-compliant client still thought it was the same track, and the title and cover stayed on the previous one. It is now derived from the track hash (with dirty characters filtered out, falling back to a placeholder path if too short).
  2. **Signal send failures were swallowed by `let _`, and a broken connection wasn't reconnected.** After a session bus restart (`systemctl --user restart dbus`, logout and back in) or the bus name being taken by another instance, signals on the old connection keep failing to send while `connected` is still true — the main thread keeps writing snapshots in, but the desktop component stays on the last frame forever, with nothing in the logs. Now three consecutive send failures mark the connection broken, set `connected = false`, and reconnect after 5 seconds. The tray side already had "reconcile and self-heal every 5 seconds"; the MPRIS side never did, and this adds it.
  3. **Position went through `PropertiesChanged`.** In the spec `Position` is a property not notified through it: it moves constantly and clients infer it by their own clock; only a jump (seek) needs to be reported — that goes through `Seeked`. What was sent earlier was `position_changed`, a non-standard usage; stricter clients receiving an unexpected property change would reset their local inference and the progress bar would jump. Now jumps send `Seeked`, and it is removed from the `PropertiesChanged` criteria.

### Diagnosis

- `KUGOU_TUI_MEM_TRACE=1` writes RSS to the log every 5 seconds along with counts of "things that might accumulate" (`logger::rss_kib` + `App::tick`): the streaming download registry, the playback queue length, the hit-zone capacity, the current track. RSS alone only shows growth; these counts narrow it to a specific path. Kept separate from `KUGOU_TUI_DEBUG`: the latter also turns on key logging, which drowns out the trend.
- Three `#[ignore]` probes, each printing per-round RSS:
  `ui::views::player::tests::cover_swap_rss_probe` (repeatedly swapping covers),
  `audio::engine::tests::file_swap_rss_probe` (repeatedly loading local files),
  `audio::engine::tests::mixed_alloc_rss_probe` and
  `mmap_threshold_effect_probe` (mixed-size allocations vs. threshold comparison).
  Run with: `cargo test --release <name> -- --ignored --nocapture`.

  Measured: 40 rounds of cover swapping, 30 rounds of loading, 40 rounds of mixed allocation — RSS settles within the first two rounds and is then **constant** — the track-change path has no object leak, and the rest is "pages not yet returned".

## [0.4.6] - 2026-09-29

### Added

- **The tray went from "register once and done" to a full integration** (`src/tray.rs`, +660 lines). Previously the tray had no menu, wheel, or status feedback, and gave up permanently after one registration failure:

  - **Wheel control**: vertical scrolling on the icon adjusts volume, horizontal seeks forward / backward. Only the delta's **sign** is used, not scaled by notches — hosts give inconsistent absolute values (some ±1, some ±120), so scaling by notches would be sometimes large and sometimes small. Three end-to-end dispatch assertions were added (scroll up / down / right), and a zero-displacement assertion that nothing is dispatched.
  - **Added "Quit" to the menu**: goes through the normal exit flow (saving config and playback position first), equivalent to pressing `q`. Previously after minimizing (niri) the only way back was to press `q` in the terminal.
  - **Play / pause and mute switch wording with state**: "Pause" while playing, "Play" otherwise; "Unmute" when muted. On a label change the menu revision increments and broadcasts `LayoutUpdated` + `ItemsPropertiesUpdated` (containing only these two ids) — both signals are sent because some hosts recognize only one. A separator was added to the menu to split playback control from the rest.
  - **The icon follows playback state**: paused / stopped switches to a **dimmed** version of the same image (alpha lowered to 45%, color unchanged) and broadcasts `NewIcon`. The loading state counts as "about to make sound" and uses the bright image like playing — a grayed icon during the two seconds of buffering actually looks stuck. Both images are pre-rendered once at startup.
  - **Registration self-heals**: the tray thread went from "give up on registration failure" to reconciling against `RegisteredStatusNotifierItems` every 5 seconds and (re-)registering when it's not in the list. Scenarios fixed: the status bar starting after the player, or a panel restart swapping the watcher — previously the tray only appeared after restarting the player, now it appears within 5 seconds at most. The log records only when the connection state **flips**, so a panel that stays away doesn't spam a WARN line every 5 seconds.
  - **Icon redesign**: a single note (notehead + stem) changed to a beamed double note (♫), the source `assets/tray.svg` redrawn, the embedded PNG raised from 64 px to 256 px; pre-rendered sizes expanded from 22 / 64 to 16 / 22 / 24 / 32 / 48 / 64 — giving fewer sizes makes some hosts hard-scale from the nearest one.

- **↑/↓ track change on the home / visualizer pages** (`MoveUp` / `MoveDown` dispatch in `app/update.rs`). These two pages have no navigable list, so the arrow keys were empty; they are exactly the "watch lyrics / spectrum while listening" pages, where track change is the most frequent operation, so it was assigned to playback control. **Pages with lists (search / playlists / artists / rankings / cloud / queue / settings) are unchanged** — taking over list navigation is taking over the core interaction. `CHEATSHEET` got an explanatory entry, and a missed item is caught by the existing keybinding-table test.

- **`audio/resample.rs`: anti-aliasing resampling of hi-res masters**. rodio's `SampleRateConverter` is **unfiltered linear interpolation** (the module's own words: "simple linear interpolation"): upsampling like 44.1k → 48k is fine, but 88.2 / 96 / 176.4 kHz masters are **downsampling** on a 48 kHz device — content above 24 kHz mirrors and folds back into the audible band, and the highs get an extra ~1 dB rolloff. It now uses rubato's sinc resampler (`sinc_len=256`, `f_cutoff=0.95`, BlackmanHarris2 window) instead of linear interpolation, and declares `sample_rate()` as the device rate, so rodio's conversion becomes an identity transform. Upsampling is not wrapped (no aliasing issue; not worth the sinc CPU). **Known boundary**: the end gets up to one extra block (~30 ms) of silence — `FixedIn`'s input blocks are fixed-length and the last partial block is zero-filled. Inaudible, and it doesn't affect the progress bar (the duration declaration comes from the inner source).

### Fixed

- **The progress bar could only be dragged forward; dragging back / seeking back / jumping back from lyrics all failed** (`audio/engine.rs::reload_at` and `audio/streaming.rs::reader_from_start`). The root cause is in the decoder: symphonia's FLAC decoder cannot seek backward in a file **without a SEEKTABLE**, and the demuxer returns `ForwardOnly`. The lossless files Kugou provides are measured to be exactly this (ffprobe finds no seektable, and a raw packet scan finds no `SEEKTABLE` block), so "dragging forward" died entirely. The fix bypasses it: **rebuild the decoder** from the same data source; the new decoder starts from the file head, so seeking to any position is a forward seek and necessarily succeeds (at the cost of one rebuild of tens of milliseconds). The streaming buffer path additionally requires "a clone that zeroes the read pointer" — the original buffer's read pointer sits at the old decoder's position, and cloning directly would make the new decoder start mid-way, and "jump back to earlier" would fall back to a backward seek.

- **hi-res masters sound "grittier" than other clients** — the resampling above fixes it. 《起风了》 is an 88.2 kHz master, and after `highpass=24k` there is still about −50 dB of real content, and its file has an **identical audio MD5** to what moekoemusic gets — the differing sound is in the downsampling step, not in file selection. Anti-aliasing is pinned by tests: a 30 kHz tone must be suppressed below −26 dB after downsampling, an in-band 1 kHz tone keeps its amplitude, and the filter state must be rebuilt after seek.

- **Clicking a lyric always jumped to the neighboring line** (the hit-zone registration in `ui/views/player.rs`). `hit_zones`'s row is a **screen row**, and `index_at` first does `row - rect.top()`; when the top of the lyric content area was taken wrong, the whole hit zone shifted by one or more rows. The fix anchors the hit zone to the **layout actually rendered this frame**: the row count is the smaller of "visible rows" and "remaining content rows", and the top is the content area top + the actual inner padding (the number of rows the content is shifted down when `offset < first_display`).
  **Measured** (a pty-driven harness, bypassing tmux's rewriting of mouse reports): after pausing, clicking the same screen row 3 times gives the same `time_ms` all three times; `row - top + first == display` is self-consistent for all 21 rows; the `display` step between adjacent screen rows is always 1; `ms` is monotonically non-decreasing from top to bottom.

- **Silent flavor-mixing when the `flac` tier delivers an mp4 container** (`api/catalog.rs`). mp4 is the output of another transcoding chain, with a different master from flac / mp3, so playing it "sounds different from other clients". Following moekoemusic's `extName == 'mp4'` check, an mp4 candidate falls through to the next tier.

- **Memory over 100 MB that doesn't fall back** (`src/main.rs` + `audio/engine.rs`). glibc uses a **dynamic** mmap threshold by default: one large allocation (cover decode, download buffer) raises the threshold to its size, and afterwards equally large blocks are carved from the arena — `free` only returns pages to the arena, and RSS stays at the historical peak thereafter. The threshold is now pinned at 1 MiB, so large blocks always go through mmap (`munmap` returns to the OS), and `malloc_trim` is called once at the natural boundary of track change. Unix only (the Windows MSVC heap already returns aggressively).

- **Tray-related docs didn't match the implementation**: `tray.rs`'s module comment and `USER_GUIDE` still said "no menu implemented" and "the watcher is only synced on process start / exit", contradicting the code that implements DBusMenu and 5-second self-healing; changed to a per-key description (left click / wheel / right click / middle click / icon state).

### Tests

This release adds 6 guard tests, each pinned to the layer where **removing the fix makes it fail immediately**:

| Test | Layer pinned |
|---|---|
| `late_song_clicks_do_not_shift_by_the_offset_gap` | Click misalignment after the last screen's `offset` hits `max_offset` |
| `hit_zone_top_anchors_to_the_first_content_row` | Hit-zone top = first content row |
| `rebuilt_decoder_can_seek_backward_from_the_start` | A rebuilt decoder can forward-seek to an earlier position and keep producing samples |
| `backward_seek_lands_near_the_target_not_at_the_start` | After zeroing the read pointer, it can jump to the target rather than back to the start |
| `ultrasonic_tone_is_suppressed_not_aliased` | 30 kHz suppressed below −26 dB by anti-aliasing |
| `in_band_tone_passes_with_its_amplitude` | In-band 1 kHz keeps its amplitude (±1.5 dB) |

Total 366 → 370 (plus 1 ignored).

## [0.4.5] - 2026-09-29

### Fixed

- **Multichannel lossless files played only the first two channels; what you heard wasn't the song** (new `audio/downmix.rs`, wired in `audio/engine.rs::build_decoder`). Trigger: playing 《东京不太热 (DJ Z新豪版)》 (`hash=1953202D07B954E785CA5249E1A64B3C`), the `flac` tier delivers a **4.0-channel** FLAC, and in that file **the song is in the last two channels**:

  | Channel | dBFS | Correlation with the same song's 128 kbps mp3 |
  |---|---|---|
  | ch0 FL | −21.0 | +0.35 |
  | ch1 FR | −20.6 | +0.39 |
  | ch2 BL | −11.7 | **+0.90** |
  | ch3 BR | −12.8 | **+0.89** |

  The correlation of all four mixed with the mp3 is +0.999 — that is, the first two channels aren't this song at all. And rodio 0.22's `ChannelCountConverter` downmixes by "keep the first N samples of each frame, discard the rest" (`conversions/channels.rs`), so we actually played only FL/FR, and the user heard a different piece of audio, showing up as "the audio version differs from other clients (moekoemusic)".

  Now both exits of `build_decoder` go through `downmix::to_stereo`: even-indexed channels → L, odd-indexed channels → R averaged, with `channels()` always 2 (so rodio's conversion becomes a no-op). Mono/stereo is an identity transform and affects the vast majority of songs not at all. **Measured** (through `build_decoder`'s real decode path, on the original files Kugou provides): the correlation of the downmixed result with the correct stereo mix rises from +0.35 to **+0.998 / +0.997**. Two guard tests: `decoded_multichannel_files_are_downmixed_to_stereo` runs a self-made 4-channel WAV through `build_decoder` and asserts `channels() <= 2`; `the_mixer_hears_the_song_only_because_we_downmixed_first` builds a soundcard-independent mixer with `rodio::mixer::mixer` and **compares**, proving "the source going straight into the mixer leaves only the first two channels" and "downmixing first lets the correct mix through" — it pins exactly the layer where the bug occurs. Both fail immediately when that layer is removed.

  **Averaging rather than summing**: summing is closer to this file's stereo master (the mp3's L ≈ `FL+BL`), but any correlation between the two channels causes clipping; averaging is inherently safe (`|out| <= max|in|`). The cost is that this pair of channels is uncorrelated (+0.03) while the master is their sum, so the downmix is about **6 dB** lower overall than the reference. Volume is user-adjustable, clipping is real distortion — take the lighter end. See the top of the `downmix` module for details.

  Fixed the same root cause elsewhere: `LevelMeter` sampled only channel 0 for level/spectrum, so with 4-channel input **the spectrum visualizer drew the wrong audio too**; putting the downmix layer in front of it fixes that automatically.

- **`/song/url`'s silent downgrade was unhandled** (`api/catalog.rs::downgrade_note`). Measured: requesting `quality=flac` for a song with no lossless tier returns `status=1` + `extName=mp3` + `bitRate=128000` (3,749,296 B), while `extract_stream_url` only checks `status` — the UI would keep showing flac while playing 128 kbps, with no way to find out. It now ranks tiers roughly by "container + bitrate" and, when the actual is lower than requested, gives a plain message (`quality downgraded: requested flac, server actually gave 128 kbps mp3`), passed to the UI via `StreamUrl::reason`. `super` and the viper series don't take part in the comparison (the interface layer has no comparable container/bitrate), and when mp3 gives no bitrate no conclusion is drawn — better to say nothing than to misreport; `mp4` / `m4a` (AAC) take part in the comparison as "lossy", so "set lossless but got AAC" is also reported (this item's basis is the reference implementation moekoemusic specifically blocking `extName == 'mp4'`, not our own measurement).
  Regression test `song_stream_url_reports_a_silent_downgrade` runs it end-to-end with a hand-written server.

- **The downgrade notice was previously swallowed entirely** (`app/update.rs`). `StreamUrl::reason` was read only in the `is_trial` branch, so the notice "viper quality has no permission, downgraded to standard" was never shown. Now the non-preview branch also reports it as an ordinary notice.

- **Candidate qualities could mix across versions** (`api/catalog.rs::parse_quality_candidates`). When resolving a stream, the multiple hashes on hand are comma-joined and sent to `/privilege/lite`, so the response has **multiple items**; the old code mixed all items' variants into one table with first-come-first-wins, and the server doesn't guarantee item order — once a hash points to **another version** of the same song (different edition / cover / ringtone), it could pick the wrong version's file. It now works in two passes: first accept only entries with the same `album_audio_id` as the primary item (identified by `hash`), and only if none are found broaden to all — the second pass must stay, as it is the fix path for "the hash from search was delisted and the song is under another hash in `audio_info`".
  Three regression tests: `candidates_never_borrow_a_quality_from_another_version`,
  `candidates_identify_the_primary_item_by_hash_not_by_position`,
  `candidates_fall_back_to_other_hashes_when_the_primary_has_nothing`.

### Changed

- **The packaging script also runs in the regular CI** (the Linux and macOS columns of `ci.yml`). The release tarball was previously produced only in `release.yml`, which runs after the tag is pushed — and **a tag can't be undone**, so if the packaging script missed a file the only options were shipping a broken package or accepting it. This project really hit that: the 0.3.7 release missed three scripts, and non-Arch users couldn't set up the interface service after extracting. Now any push builds a release tarball and uploads it as an artifact, so problems surface before the tag is pushed; the macOS column additionally covers the branches written in `make-release-tarball` for BSD tools (`readlink`, `shasum`) — those only execute there.

- A comment in `api/model.rs` about "splitting `audio_id` into two fields stored separately and trying them one by one when resolving the stream" didn't match the implementation (there is only one field, and `/song/url` deliberately sends no album-family id). Rewritten to describe reality, so the next person doesn't go looking for a retry logic that doesn't exist.

## [0.4.4] - 2026-09-28

### Changed

- **Split `app/update.rs`** (4728 → 4144 lines). It is the only place the whole program changes state, and it had grown to 4700 lines, so changing one thing required reading too much context. Three blocks were split out, **all pure moves with zero logic change**, compared byte-for-byte after normalizing visibility (the `diff` output for `search` / `playback` is empty, and `navigation` differs by one trailing blank line):

  | New module | Lines | Contents |
  |---|---|---|
  | `app/navigation.rs` | 237 | Page switching, focus, selection movement, `go_back` |
  | `app/playback.rs` | 320 | Start / track change / progress / volume / mute / lyric offset |
  | `app/search.rs` | 142 | Submit search, load more (including `SEARCH_MAX_PAGES`) |

  **Boundaries matter more than headings.** The "playback control" section held five methods that aren't about playback, and they did not move with it:
  `sync_mpris` / `sync_tray` (output adapters called by `tick()` every beat), `toggle_window`
  (niri's compositor IPC), `client_for` (a client-construction helper shared by the whole file, 6 call sites),
  `request_lyric` (its result handling is in `handle_loaded`, and keeping request and handling in the same file saves context).
  The criterion is "where the callers are, who handles the result", not "which heading it sits under" — better a smaller module with a clean boundary than dragging neighbors along to pad the size.

  **After the split, reviewing dependencies found and fixed a leaf-to-leaf cycle.** `activate()` (the Enter key deciding by `(Tab, Focus)` whether to search, play, or open) was originally moved with "navigation" into `navigation.rs`, so navigation called search and playback, making `navigation ↔ search` bidirectional. `startup_search()` (the `--search` startup glue) likewise made search depend back on navigation. After both moved back to `update.rs`'s dispatch layer, the dependencies became a star: `update` calls the three modules one-way, `navigation` / `playback` only call back to `update`, and `search` calls no one.
  **A move being equivalent doesn't mean the boundary is correct — a wrong boundary doesn't error, it just moves the coupling from within a file to between modules**, so after each block is split, recount the module call graph.

  The cost stated plainly: Rust's method privacy is per "module where the `impl` block is written", not per type — after a method moves, calls from the other side report `E0624` and must each be changed to `pub(super)`, and **both directions need changing**. This is the inherent cost of splitting, not a design flaw; the compiler lists them all.

  The remaining `cloud.rs` / `settings.rs` continue at the same granularity, one block at a time. The rules and progress are in `docs/MAINTENANCE.md` §1.8.

- **Split `app/update.rs` round two** (4144 → 2958 lines), finishing the `cloud.rs` / `settings.rs` mentioned above plus a `desktop.rs` block. Again pure moves: 116 functions compared one by one by signature + body (whitespace collapsed, `pub(super)` normalized) **all surviving byte-for-byte**, the only deletion being one duplicated implementation (see the first item under "Fixed" below).

  | New module | Lines | Contents |
  |---|---|---|
  | `app/cloud.rs` | 937 | Login / source switch / VIP / cloud playlists / account profile |
  | `app/settings.rs` | 728 | Settings page: candidate values + display text + three interactions |
  | `app/desktop.rs` | 106 | MPRIS / system tray / window control |
  | `app/navigation.rs` | 275 | Additionally took on number keys and the `v` key opening the source page |

  Dependencies are still a **star** with no mutually-calling pair: `update` points to 6 modules, and modules only point back to `update` / `mod`. One **deliberately accepted** cross-module call: `navigation` calls `settings`'s `move_settings` (settings-page cursor movement belongs to the settings page; the reason is in MAINTENANCE §1.8).

- **Fixed four misplaced doc comments.** Their common symptom is "a comment sits right before another function's doc and is merged into the same doc by Rust", so `fetch_user_info`'s doc opened talking about VIP, `handle_digit`'s about the `v` key switching sources, `open_quality_picker`'s about downloading songs, and `describe_song`'s about quality labels. All four returned to their places.

### Added

- **`scripts/kugou-api.ps1`: service management on the Windows side** (start / stop / restart / status / logs). Previously there was no way to stop the background node on Windows: the launcher only did "probe → launch if not running", so stopping meant guessing by name in Task Manager, and guessing wrong took other Node projects down with it. Now both platforms share the same PID-file convention (`<PID> <port>`, space-separated, in the cache directory as `api-<instance>.pid`), so an instance the launcher started can be stopped by `kugou-api.ps1 stop`.

- **`kugou-api logs`**: follows the log directly (`logs lite` follows the concept edition). Previously you had to `status` to get the path and then `tail` yourself.

- **The launcher gained `--dry-run`** (bash side; the PowerShell side already had it): prints only the parsed config, source, instance, probe address, service directory, and binary path, and starts nothing. Debugging "why does it say the service isn't running" no longer needs guessing.

- The launcher now **checks for `node`** before launching the service. Previously, without Node installed you waited the full 20 seconds and then inferred the cause from the `ENOENT` at the end of the log.

- On macOS the launcher checks the binary's `com.apple.quarantine` attribute and gives the `xattr -d` command. When a signature-less binary is killed by Gatekeeper the terminal only shows `Killed: 9`, which looks like the program's own bug.

### Fixed

- **NetEase playlists / rankings were silently truncated** (`source/netease.rs`). `playlist_tracks_all` stopped paging on "less than a full page ⇒ done", while `song_from_json` filters entries (missing hash, wrong field type), so 500 entries leaving 499 is common — and once hit, the whole table stops there, and the user sees a shortened playlist with no error. The Kugou side (`api/catalog.rs::collect_all_pages`) has always been "stop only on an empty page"; the inconsistency between the two criteria was itself the signal of this bug.

  Changed to "stop only on an empty page", with the criterion extracted into the free function `should_continue_paging` (`got > 0`, and the signature **deliberately has no** `page_size` — once page size enters the criterion, truncation returns).

  The premise for changing it was "does an out-of-range offset return an empty array or an error", which couldn't be measured before (only the Kugou concept-edition service `:3001` was running locally). This time NeteaseCloudMusicApi was started (`:3002`) and measured:
  `/playlist/track/all?id=3778678&limit=500&offset=500` (that playlist has 200 songs) and
  `offset=999999` both return `{"songs":[],"privileges":[],"code":200}` — **an empty array, not an error**, and it doesn't clamp the offset back to the last page and return duplicate content. So one extra out-of-range request is safe, at the cost of one extra request at the end of a full-table load (this interface takes several seconds per call).

  Regression test `paging_stops_only_on_an_empty_page`: it fails on the spot when the criterion is reverted to `got >= 500` (`1 条也该继续翻页`). Another, `an_out_of_range_page_is_empty_and_
  therefore_ends_paging`, pins the **measured server-behavior conclusion** above — if upstream one day returns a non-200 for out-of-range, `playlist_tracks_all`'s `?` will throw the error up, and this test must be re-measured and changed then. `docs/MAINTENANCE.md` §5, §7 synced.

- **`;` (open the track context menu) couldn't be rebound** (`keymap.rs`). It appears in both `resolve_normal` and `CHEATSHEET`, the help panel shows it, and `docs/CONFIGURATION.md`'s keybinding table lists it, but the action-name table in `action_from_name` **lacks `context_menu`** — a user writing `context_menu = "m"` per the docs got "unknown action context_menu", while the action clearly exists. `CONTRIBUTING.md` lists "remember to sync this table" as a manual step, and this time it was missed.

  The action-name table changed from a `match` to `const REBINDABLE` (data), so "do all actions with a default key in the help panel have a bindable name" can be checked mechanically: a new test compares the `CHEATSHEET` and `REBINDABLE` lists, and removing the fix makes it report precisely `「;」（打开歌曲右键菜单）→ ContextMenu`. A self-check of the table itself (no duplicate names, all lowercase snake_case) was added too — duplicates make `find` silently take the first. `docs/CONFIGURATION.md`'s list and count (60 → 61) follow.

- **Lyric translations shifted by one line overall** (`api/lyric.rs`). `parse_lrc` and `attach_translations` each wrote their own "does this line count as a lyric" criterion: the former checks whether `parse_krc_words`'s return is empty, the latter whether `clean_krc_markup` is empty. They diverge on a line with "text before `<` but no matching `>`" (like the `宝贝<3` written in the song), so `attach_translations` computed a line number one too high, and **every subsequent line's translation shifted by one** — the user sees "the translation doesn't match the original", with no error.

  The criterion now exists in only one place (`has_lyric_content`), and both call it. The same root cause's other symptom is fixed too: `parse_krc_words` only emitted the characters **after** the marker, so that line was previously dropped entirely — text before `<` is also body text and is now kept (those characters have no per-character timing, so it falls back to whole-line highlight).

  Regression test `a_line_with_a_stray_angle_bracket_does_not_shift_translations`:
  removing the fix makes it report `left: [译1, 译2, 译3]` / `right: [译1, 译3, 译4]`.

- **The cover "no distortion" mode panicked on a degenerate area** (`ui/views/player.rs::fit_box`).
  `rows.clamp(1, area.height)` blows up directly when `area.height == 0` — `Ord::clamp` requires
  `min <= max`, and `clamp(1, 0)` triggers `assert!(min <= max)`.

  There is now **no reproducible trigger path** (measured: `render_home`'s layout gives the cover column at least 4 rows for any height between 4 and 24, with `Min(6)` taking priority over `Length(8)`), but that is a coincidence of the layout, not a guarantee here — the cover area's layout itself changed once (see that comment "previously let the cover box compute its own height from the image ratio"). Now a degenerate area returns an empty rectangle: it neither panics nor pushes the box outside the area (forcing 1×1 would make `area.width - columns` underflow on the spot). The regression test pins both "zero size doesn't panic" and "the computed box never exceeds the area".

- **The PowerShell scripts on Windows show garbage everywhere** (`scripts/*.ps1`). None of the four `.ps1` files had a UTF-8 BOM, and Windows' built-in Windows PowerShell 5.1 **decodes by the current ANSI code page** when there is no BOM — GBK on Chinese Windows — so the dozens of user-facing Chinese messages in the scripts ("node not found", "already running"…) all became garbage.

  Why it went unnoticed: the Windows column of CI uses `shell: pwsh`, i.e. PowerShell 7, and 7.x reads UTF-8 by default — **CI being green doesn't mean it's fine for the user**. All four files now have a BOM (only the decoding changed, verified byte-for-byte that nothing except the first 3 bytes moved), and this is written into `docs/MAINTENANCE.md` §1.7.

- **Concurrent paging produced a random full-table order** (`api/catalog.rs`). `collect_all_pages` sent a batch of pages at once and then directly did `all.extend(songs)` in `JoinSet::join_next()` — and `join_next()` gives the **task completion order**, not the page order. So opening any playlist / ranking / artist page over 6 pages (>180 tracks) had a scrambled page order. Rankings are especially fatal: their "order" is the ranking content itself, and TOP500 randomly permuted means the ranking is gone.

  Now a batch's results are collected into `collected` by page number and spliced after sorting. The regression test uses a hand-written fake server to make pages return **necessarily in reverse order** (page N waits `60ms × (7 - N)`), so this test doesn't pass by luck: removing the sort makes it steadily produce `h1, h7, h6, h5, h4, h3, h2`.

- **Two streaming downloads could run on the same song at once** (`audio/download.rs`). Pressing Enter twice is two `start_streaming` calls landing on the same target (`App::active_stream` is only held once the pre-roll is reached, and nothing stops the second one in that window). Two streams destroy each other: the later one's `truncate(true)` wipes the earlier bytes from disk (while data outside the buffer window is read back via `pread` on that file, so reading a hole means noise), and the `remove_file` on failure / cancel deletes the other's file, making its rename fail and wasting a whole download.

  Now `Downloader` registers running streams by temp-file path, and a second one reuses the first's buffer instead of starting a task. Also, whole-track and streaming downloads use separate temp files (`x.mp3.part` and `x.mp3.stream.part`) — when prefetching the next track in the background, the user may well switch to exactly that one.

- **Two versions of the quality labels.** `app/update.rs` had another private `quality_label` recognizing only 6 tiers, missing `viper_atmos` and `viper_tape` compared to `app/settings::quality_label`. So switching to those two tiers with a shortcut showed the raw string `viper_atmos` in the status bar, while the settings page showed the same item as "viper panorama". Now there is only one implementation, plus a test: every selectable quality must have a display name, and may not fall back to the raw string.

- **The "N songs" on the NetEase artist page was the album count.** `source/netease.rs` put `/top/artists`'s `albumSize` into `song_count`, but that's the album count; the song count is `musicSize`. Correct type, wrong meaning, and every artist on the UI carried a wrong number with no exception. Changed to `musicSize`, and when unavailable it shows nothing (better than showing a wrong number).

- **Chunked concurrent download could write a corrupt cache file** (`audio/download.rs`). Chunked download only checked `status.is_success()`, so when the server (or an intermediate CDN / proxy) ignored `Range` and returned `200 OK` with the whole content, it still wrote: the four tasks each `seek(their own start)` and wrote the whole thing, overwriting each other, producing a "correct length, entirely wrong content" file that was still renamed into the cache — and every later play hit it.

  Now three things must hold simultaneously before continuing to write: the status code must be `206`, `Content-Range` must parse, and the parsed range must be **byte-for-byte equal** to the requested range (also checking the total length matches HEAD). If any fails, it falls back to a single-connection re-download — better one slow time than one corrupt cache file.

- **An incomplete download was treated as success.** The completion condition was previously "bytes written greater than 0", but `.part` has already been `set_len(total)` pre-allocated, so the file size itself proves nothing about completeness. Now each chunk must fill its own segment, the total must equal the total length, and `sync_all` is called before the rename; the single-connection path also requires receiving the full amount when the server gives `Content-Length`.

  The regression test starts a minimal HTTP server with std's `TcpListener` (no new dependencies), covering four cases: normal 206, server ignoring Range and returning 200, `Content-Range` not matching the request, and a mid-transfer interruption. Removing the three guards one by one makes the "server ignores Range" case return 4 MiB (real length 1 MiB) — the corrupt file from before the fix.

- **The Windows release zip had no PowerShell scripts.** `build-windows.ps1` copied only three bash scripts, while README and `docs/INSTALL.md` tell Windows users to run exactly `.\scripts\kugou-tui.ps1` / `.\scripts\kugou-api-install.ps1` — those files don't exist after extracting, so the first step is stuck. Now the rule is "whatever the docs mention must be in the package", and all six scripts are included.

- **Three bash scripts wouldn't run on macOS.** Listed item by item, because each one aborts the script directly:

  - `declare -A` (`kugou-api`, `kugou-api-install`) needs bash 4, while macOS's built-in `/bin/bash` is 3.2 — it reports `declare: -A: invalid option` and not one line runs. Changed to `case` dispatch + indirect expansion.
  - `readlink -f` (all three scripts) is a GNU extension, and macOS's `readlink` reports
    `illegal option -- f`. Changed to resolving symlinks level by level (`make-release-tarball` already did this).
  - The config and cache directories were looked up in the wrong place on macOS: `dirs` gives
    `~/Library/Application Support` and `~/Library/Caches`, while the scripts read `~/.config`
    and `~/.cache`. The consequence is that the launcher never finds `config.toml` and always probes with defaults (`:3000`, standard edition) — people using the concept edition only see "service failed to start".
  - `setsid` (util-linux specific) and `ss` (iproute2) don't exist on macOS, falling back to
    `nohup` and `lsof` respectively.
  - `seq` replaced with a `while` counting loop.

  > Also unified the "config root directory" to `KUGOU_TUI_CONFIG_DIR`: the PowerShell side always honored it, while the bash side previously honored only the XDG variables.

- A service launched by the launcher now writes a PID file, so `kugou-api stop` / `status` can manage it. Previously it was an "orphan process": the port was taken, but it couldn't be stopped by PID file, and `status` only said "the port responds, but it wasn't started by this script".

- **Late async results polluted the UI** (`app/update.rs`). All requests are `tokio::spawn`ed and return in no guaranteed order, and previously only the "playback" side checked identity (`is_current`, lyrics/cover by `hash`). The rest took results as they came, so:

  - Search "A" → results still in flight → search "B" → A's results arrive first and **overwrite B's results entirely**; pressing `M` to load more is worse: A's songs are appended to B's list while the title still says B.
  - Open playlist A → user opens B instead → A's results arrive first, and the right side becomes "title says B, content is A".
  - Same for artists and rankings (left column highlights B, right column is A's songs).
  - When switching the playlist-plaza category / artist region filter, the old category's results overwrite the new category's list.

  Now every result must prove its identity on return and is discarded if inconsistent; the criteria are extracted into two free functions, `accepts_search_result` and `accepts_open_item`, with tests (`App` construction needs an audio engine and runtime, which can't be built in unit tests, so the criteria must live where they can be tested alone).

  Two same-root minor problems fixed too: a stale result no longer wipes the in-progress search's "searching…" (clearing `busy` moved after the criterion); and pressing `M` repeatedly no longer fires one request per press on the same `page` (both results appended means a duplicated page, and the later one also reverses the order).

  Identity fields are always written to state **before** `spawn` (`open_playlist` / `open_artist` / `open_board`); writing them in the result is too late — that result never matches. The criteria and list are in `docs/MAINTENANCE.md` §4 item 11.

### Changed

- **Pushing a tag alone is enough to release.** `release.yml` gains a first job `prepare` that calls the new `scripts/release-notes` to extract the version's body from `CHANGELOG.md` and create the Release; all three platform jobs `needs: prepare`.

  Previously the Release could only be created by the local `scripts/release`, and the workflow only **waited** for it to appear — so "push the tag alone" didn't work: pushing the tag starts CI, and after waiting a full 5 minutes with no Release, all three jobs failed together. Now `git tag … && git push origin <tag>` does it in one path.

  `scripts/release-notes` is extracted from that `awk` block in `scripts/release`: this body has two consumers (local release and CI), and writing it twice necessarily drifts, while the release notes are the face of the release page.

  > Cost: pushing the tag alone doesn't update AUR's `PKGBUILD` / `.SRCINFO`, nor validate that CHANGELOG has the corresponding version section. To also release AUR, you still run `scripts/release`.

- **The Linux release package is also produced by CI.** Previously only Windows and macOS were handed to CI, and the Linux one was built and uploaded by the local `scripts/release` — which meant **no Linux package without the dev machine at hand**: switching machines, reinstalling, or just backfilling assets for an already-released version all required rebuilding the Rust toolchain and the entire dependency tree. Now `release.yml` gains an `ubuntu-latest` column, and pushing a `v*` tag builds all three platforms uniformly, with artifacts independent of any specific machine.

  The local `scripts/release`'s packaging and upload are **unchanged**: the two sides share names and `upload-release-asset`'s `--clobber` overwrites, so whoever arrives later wins. Therefore the two sides' sha256 are **not guaranteed identical** (the rustc minor version and the target machine's C toolchain both affect `aws-lc-sys`'s compilation result), and a locally recorded hash not matching later is normal.

- **CI's Linux column uploads a binary artifact** (`kugou-tui-linux-x86_64`, kept 14 days). Previously every push ran `cargo build --release`, but the artifact was discarded after use — getting a runnable binary meant waiting for a release or rebuilding locally (`lto = "fat"` + `codegen-units = 1`, several minutes).

## [0.4.3] - 2026-09-27

### Added

- **Windows and macOS release packages.** Previously the Release had only the Linux tarball — not an omission, but because the dev machine is Linux, and Windows's `.exe` must be built on Windows (MSVC toolchain) and macOS's binary on macOS (Apple SDK). Now it's handed to CI:

  - `.github/workflows/release.yml`: on pushing a `v*` tag, builds one each on `windows-latest` and
    `macos-latest`, packages them, and attaches them to the GitHub Release;
    `workflow_dispatch` can also specify a tag manually to backfill assets for an **already-released** version.
  - `scripts/upload-release-asset`: the upload step extracted on its own, shared by both platforms (on a Windows
    runner, `shell: bash` is Git Bash). It handles two things: **waiting for the Release to appear**
    (the local release flow is "push tag → create Release", and pushing the tag starts CI, so uploading directly would hit
    release not found), and **idempotency** (`--clobber`, overwriting same-named files on backfill/retry).
  - Both jobs first verify that `--version` matches the tag, catching a wrong checkout ref.

  > Linux is not rebuilt here: `scripts/release` has already built, verified, and uploaded it locally.

- **Lyric line-change transition animation** (imitating Apple Music's per-line switch). Previously a line change was an **instant jump**:
  the moment `active_line` changed, the colors and scroll offset switched hard together, with no easing in the time dimension.

  The approach replaces "how far from the current line" from an integer distance with **an interpolation of two anchor distance fields** — on a line change the old anchor is kept,
  and by `t = ease_out_cubic(elapsed / total)` it interpolates between "distance to the old line" and "distance to the new line":

  ```text
  d = lerp(|line - old_line|, |line - new_line|, t)
  color = mix(mix(text_dim, lyric_far, fade(d)), highlight, heat)
  ```

  `heat` is a single scalar expressing both entry and exit: the new line `0 → 1` lights up, the old line `1 → 0` fades out, and the other lines'
  brightness levels rearrange smoothly with it. **No new render primitives were added**; it all reuses the existing `mix` /
  `fade_ratio`, with the per-character sweep layered on top of `heat` as-is.

  > **Why no "slide up" or "zoom"**: terminals have no sub-cell positioning — one character cell is one row,
  > `Paragraph`'s scroll offset is an integer (no fractional scrolling), and font size is fixed. So the position dimension
  > can't move; the only dimension available for a transition is color. Forcing a whole-line jump would just look stiff.

  - Duration adapts to line spacing: `min(configured value, gap to the next line × 0.55)`. Fast songs' lines last only a few hundred
    milliseconds, and going by the cap would mean "the previous transition hasn't finished before the next line is due".
  - Crossing 3+ lines at once (dragging the progress bar, clicking a lyric line to jump) **snaps** directly, without fading line by line.
  - Changing tracks / clearing lyrics resets the transition state, so a new song's first line doesn't fade in from some line of the previous song.
  - Reuses the existing 30fps speedup logic (`lyric_visible`), **adding no timer for the animation**.

- **Click a lyric line to jump to that line.** The hit-zone machinery (`HitZone` / `HitTarget`) was ready to use; adding
  `HitTarget::LyricLine` suffices; the "display row → lyric line" mapping is backfilled each frame by the render layer, under the same convention as
  `hit_zones` (only the render layer knows the line geometry — translations/transliterations break the one-to-one correspondence).

  The jump target must **add back `lyric_offset_ms`**: the current line is computed as `position − lyric_offset_ms`,
  so jumping directly to `line.time_ms` would be off by exactly one line. Missing this doesn't error, so it's extracted into
  `lyric_seek_target` with a test.

- The `lyric_anim_ms` config item (default `200`, `0` = off), with a "lyric animation" item on the settings page.
  Three cases force it off regardless of value: `lite_mode`, 16-color mode (no intermediate levels, so a fade
  degenerates into "a hard flip at the halfway point"), and crossing 3+ lines at once.

### Changed

- `scripts/make-release-tarball` was changed to **also run on macOS** (CI's macOS column needs it to
  produce the `aarch64-apple-darwin` package). Two GNU-specific things were replaced: `readlink -f`
  (BSD's readlink has no `-f`; changed to resolving symlinks level by level) and `sha256sum`
  (on macOS it's `shasum -a 256`). Also confirmed it uses only syntax available in bash 3.2
  — macOS's built-in bash is 3.2.

- `fade_ratio`'s parameter changed from `usize` to `f32` (during a transition the distance is an interpolated fraction).
  In steady state the value passed in is still an integer, and the result is **bit-identical** to before — so the existing color assertions are unchanged, with one
  extra guard test watching that "after the transition finishes, it matches steady state cell by cell".

### Verification

- `cargo test` **313 passed / 0 failed** (20 added this time)
- New tests cover: progress endpoints and ease-out monotonicity, the four vetoes, line-spacing adaptation, last-line fallback,
  mid-transition redirection, display-row mapping out of bounds, **both ends of the cross-fade correct** (old line dims / new line brightens),
  both lines half-lit mid-transition, transition-end matching steady state cell by cell, placeholder notices not registering hit zones,
  and the jump target adding back the offset (three offset levels + underflow)
- `cargo fmt --all --check`, and `clippy -D warnings` on both the Linux and Windows targets, all exit 0
- `scripts/make-release-tarball` was actually run (including a symlink scenario), and
  `scripts/upload-release-asset`'s "glob with no match must error" branch was measured to exit 1
  — this caught a real bug: `assets=("$pattern")` with quotes means bash doesn't do path expansion,
  so that check never fired, and CI would be all green while the Release had nothing

## [0.4.2] - 2026-09-27

### Added

- **Windows support (10/11, x86_64).** Previously `docs/INSTALL.md` simply wrote Windows off as
  "doesn't work", on the grounds that `zbus` needs `async-io` there; in practice the problems went beyond that one, and
  a batch was fixed at each of three layers, "doesn't compile → compiles but behaves wrong → behaves right but is awkward to use":

  1. **Doesn't compile** (three, all hard errors):
     - `audio/streaming.rs` used `std::os::unix::fs::FileExt::read_at` (`pread`).
       Windows has no `pread`; a `read_at` wrapper was added, going through `seek_read`
       (borrowing `ReadFile`'s OVERLAPPED for a positioned read) there. The side effect is that it moves the file cursor,
       and that handle is a read-only spill file whose cursor no one uses, so it's harmless — noted in the comment.
     - `logger.rs`'s stderr redirection used `libc::dup2` + `STDERR_FILENO`,
       while `libc` on Windows **has no** `STDERR_FILENO`, and the `std::os::fd` module
       doesn't exist either. Changed to a platform branch: Unix keeps `dup2`, Windows uses `SetStdHandle`
       to change `STD_ERROR_HANDLE`.
     - `mpris.rs` / `tray.rs` are D-Bus integrations, and Windows has neither a session bus
       nor a host recognizing these two interfaces. Together with the `zbus` dependency they're confined to `cfg(unix)`,
       with the related fields and sync logic conditionally compiled.

  2. **Compiles but behaves wrong**:
     - **`~` expansion in the download directory fails on Windows**. `app/settings.rs` read only `HOME`,
       a Unix convention, while on Windows it's `USERPROFILE` — the result was that `~/Music` stayed as-is, ending up with a directory literally named `~`, and the user not finding files where they expected.
       Changed to `HOME` first, then `dirs::home_dir()` if unavailable (on Windows it goes through
       `SHGetKnownFolderPath`). Also accepts the backslash form `~\Music`.
     - **Config write location**: `dirs` on Windows gives `%APPDATA%` / `%LOCALAPPDATA%`,
       needing no platform branch, but the docs only ever listed XDG paths, making it easy to look in the wrong place.
     - **`--print-config` lied about tray status**: on Windows it showed "system tray : enabled",
       while that code isn't compiled at all. It now honestly shows "unavailable (windows has no D-Bus)",
       and `tray`'s default is now platform-dependent (off by default on non-Unix).

  3. **Dependency adjustments**:
     - The TLS backend forks by platform: non-Windows stays rustls, **Windows switches to the system's built-in
       SChannel**. This isn't a performance trade-off — rustls drags in `aws-lc-sys`, which is C code,
       and on Windows that needs **CMake + NASM** installed in addition to MSVC to compile. After the swap,
       Windows's build prerequisites are just "Rust + MSVC toolchain", and certificate validation additionally goes through the system root store.
     - `zbus`, `libc` confined to `cfg(unix)`; the Windows side gains `windows-sys`
       (already in the dependency tree, just explicitly declaring the features `SetStdHandle` needs).

- `scripts/build-windows.ps1`: Windows's build + packaging script (the Unix side's counterpart is
  `make-release-tarball`), producing `dist\kugou-tui-<version>-<triple>.zip`.
  The rest of the repo's scripts are bash and don't run on Windows by default; the build itself is two commands, and
  the truly easy-to-miss part is packaging (the release package must carry those three bash scripts and `docs/`).

- `scripts/kugou-tui.ps1` and `scripts/kugou-api-install.ps1`: the Windows-side launcher
  and service installer, counterparts of the same-named bash scripts. After installing, daily use is two commands
  (`kugou-api-install.ps1` once → `kugou-tui.ps1` to start playing), no longer needing to manually open a terminal
  to start `node app.js`. A few deliberate trade-offs:

  - The launcher **has no `param()` block**: once parameters are declared, PowerShell treats `-s 海阔天空`
    as a misspelled parameter name and errors out; without `param()`, all arguments go into `$args`, which passes them through as-is.
  - Install and start are **separate** (on the bash side the installer starts the service along the way): this way "how to start the service"
    has only one implementation, in the launcher, and the two sides don't drift.
  - Depends only on PowerShell **5.1** (the version Windows ships), requiring no extra pwsh 7;
    platform detection uses `$env:OS` rather than `$IsWindows` (the latter only exists from 6.0).
  - Has `--dry-run`, printing only "current source / probe address / service directory / is the service running",
    so debugging "why does it say the service isn't running" needs no guessing.

- **CI** (`.github/workflows/ci.yml`): **Linux / Windows / macOS** each run
  `clippy -D warnings + test + build`, and the Windows column runs the packaging script once more.
  Previously the repo had no CI, yet `CONTRIBUTING.md` said "CI uses the same commands" —
  now that statement holds. The Windows column is this time's focus: path expansion, config directory,
  and cache file naming only surface when actually run, and `cargo check` alone can't guarantee them.

- **macOS entered CI**, so "does macOS work" now has a verifiable answer instead of "theoretically possible".
  At the code level it shares the `cfg(unix)` branch with Linux and had no Linux-specific call residue;
  but **sharing isn't the same as verified** — `dirs` there gives `~/Library/...` instead of XDG directories,
  audio goes through CoreAudio, and font detection can't reach `fc-list`; these are only visible when the tests actually run.
  Known differences (no MPRIS/tray, no minimize window, different config paths) are in `docs/INSTALL.md`'s
  new "Building and running on macOS".

- `KUGOU_TUI_CONFIG_DIR`: overrides the config root directory wholesale. Useful for portable installs (program and config together on a USB stick);
  it also lets the "config survives a restart" test run on Windows — there `dirs` goes through the Win32 Known Folder, and `XDG_CONFIG_HOME` has no effect on it, so without this switch the test could only be skipped.
- `KUGOU_TUI_NERD_FONT`: explicitly specifies whether a Nerd Font is present. Windows has no fontconfig,
  so `fc-list` detection always fails; users with a Nerd Font use this to turn the icons on.

### Changed

- **Corrected the wrong MSRV declaration: `rust-version` 1.86 → 1.90**. Declaring 1.86 was wrong, and harmfully so — building with 1.86 yields a long list of "some dependency requires a higher rustc" rather than "you need 1.90".
  The real floor is pushed up by a **dependency** (`quantette` 0.6 requires 1.90, pulled in via ratatui-image → icy_sixel;
  ratatui 0.30 requires 1.88, rodio 0.22 requires 1.87), confirmed with `cargo +1.90.0 check --locked
  --all-targets`. The old number in the README badge, `docs/INSTALL.md`, `CONTRIBUTING.md`, and
  three source comments was changed too.

- **Cleared 29 clippy suggestions exposed only by the real MSRV**. This isn't "incidental cleanup" — that wrong
  MSRV declaration also suppressed all of clippy's MSRV-aware lints (it uses `rust-version` to decide which
  new APIs are available), so the moment the declaration changed to 1.90, `clippy -D warnings` went red. Three kinds:

  | lint | Count | Fix |
  |---|---|---|
  | `collapsible_if` | 23 | `if let Some(x) = a { if b { .. } }` → `if let Some(x) = a && b { .. }` (let-chain, available since 1.88) |
  | `manual_is_multiple_of` | 3 | `n % 2 == 0` → `n.is_multiple_of(2)` |
  | `chunks_exact_to_as_chunks` | 3 | `bytes.chunks_exact(4)` → `bytes.as_chunks::<4>().0` (element type goes from `&[u8]` to `&[u8; 4]`) |

  All are machine-applicable suggestions from `cargo clippy --fix`, semantically equivalent, and all 293 tests pass after.

- **`cargo fmt --all` flattened the format baseline**, so CI can now run `cargo fmt --check`.
  The repo had 38 spots that weren't rustfmt-clean (mostly hand-written line breaks differing from rustfmt's choices),
  while `CONTRIBUTING.md` required contributors to run `cargo fmt --all` — the two didn't match.
  Flattened this time (19 files, pure whitespace), so that "CI uses the same commands" truly holds.

- The `~` expansion logic is extracted into a pure function (the home directory passed in by the caller), so related tests no longer rewrite the process-level
  `HOME` — tests run in parallel, and that pattern is a latent flake.
- The audio-device open failure message gives platform-specific troubleshooting hints (on Windows it mentions the Windows audio service,
  rather than always saying "check PipeWire/ALSA").
- `docs/LICENSES.md` recomputed against the new `Cargo.lock` (dependencies 469 → **478** packages;
  permissive licenses 455 → **464**). The new `native-tls` / `openssl` / `openssl-sys` etc.
  are the OpenSSL backend `native-tls` declares for "non-Windows, non-macOS" targets —
  this project uses it only on Windows (going through SChannel), and **a Linux build doesn't compile these packages at all**;
  the doc notes this so readers of the table don't think the artifact gained an OpenSSL.

### Docs

- README changed from "a Linux project" to a **multi-platform narrative**: the install section is split by platform (prebuilt package / source build),
  plus a Linux / Windows / macOS capability comparison table. Badges synced (`platform` and `rust` versions).
- `docs/INSTALL.md`: added "Building and running on Windows" (including PowerShell script usage,
  `--dry-run`, terminal requirements, feature-differences table, and how to check you haven't broken Windows) and
  "Building and running on macOS"; the requirements table gains Linux's `alsa-lib` / `pkg-config`
  prerequisites (missing on a clean container, it errors in `alsa-sys`, previously unstated) and a macOS row.
- `docs/FAQ.md`: added a "Platform-specific" section, answering Windows's scripts/toolchain/tray/cover/fonts
  and "does macOS work".
- `docs/RELEASE.md`: explains that the Windows zip is produced by `scripts/build-windows.ps1`,
  **currently not attached automatically by `scripts/release`** (that script is bash, running on Linux),
  plus the manual steps to attach it to the Release and the future plan to hand it to CI.
- `docs/CONFIGURATION.md`: **the action-name list was missing `toggle_window`** (wrote 59, actually 60),
  now added with a note that it is the only action with no default key and only takes effect under niri; the `tray` section gains
  the non-Unix platforms' behavior.
- Corrected "minimize window = `m` key": `m` is **mute**, and minimize's entry is in the **tray menu**
  (three places: README / INSTALL / CHANGELOG).
- `docs/MAINTENANCE.md`: §1.6 gains a macOS column and a note on the three-platform CI; new §1.7
  records the Windows-side three scripts' pitfalls (`param()` vs `$args`, `Start-Process`'s
  redirection limit, using only 5.1 syntax).

### Known limitations

- On Windows there is no system tray, no MPRIS, and no "minimize window" (its entry is in the tray menu):
  the first two are D-Bus interfaces, the last goes through niri's compositor IPC. All are **degradations**:
  the entries don't appear, and nothing does nothing when clicked.
- On Windows the cover can only be half-block character art: Windows Terminal doesn't support the Kitty / iTerm2 graphics protocols.
- The Windows-side `kugou-tui.ps1` / `kugou-api-install.ps1` are **counterparts, not line-by-line ports** of the bash versions:
  they cover only the main flow (current source → service directory/port/`platform` → probe → launch →
  wait ready → enter the player), and `kugou-api` (the `start`/`stop`/`restart`/`status` subcommands)
  has no counterpart — the launcher already covers its only daily use. To stop the service, close the player and
  `Stop-Process -Name node`, or use `Get-NetTCPConnection -LocalPort 3000` to find the PID.
- **The Windows zip isn't in the release flow yet**: the Release currently has only the Linux tarball, and
  Windows users build from source per `docs/INSTALL.md`. To attach it, run
  `build-windows.ps1` once on Windows and then `gh release upload`, or have CI upload on tag.
- **macOS hasn't been used long-term on real hardware**: CI runs `clippy + test + build`, which doesn't cover
  audio device enumeration, CoreAudio actually making sound, or the iTerm2 graphics protocol.
- **No prebuilt package for Intel Mac**: CI's `macos-latest` is arm64, and the produced binary can't run on
  Intel machines. Intel users can currently only build from source (`cargo build --release`).

## [0.4.1] - 2026-09-26

### Fixed

- **The program hogged the sound card as soon as it ran, silencing the microphone and speakers; sound didn't come back after exit**.
  This is a bug that took down the whole system's audio, and the root cause is "bypassing the sound server and connecting directly to hardware":

  1. **The settings page had a device named "Default Audio Device", which looks the most like "system default",
     but is actually ALSA's `sysdefault` — a direct alias of `plughw:0`** (see
     `/usr/share/alsa/pcm/default.conf`; the real default device is called
     "Default ALSA Output (currently PipeWire Media Server)"). Selecting it made the program connect directly to the USB sound card, and PipeWire then trying to open that card got "device or resource busy" —
     the microphone, speakers, and other apps' sound all went away.
  2. Even without selecting it, `open_default_sink()` would **iterate the device list and grab the first one it could open** when the default device couldn't open — which on a server system is likewise grabbing the hardware.

  The engine now first probes whether a sound server (PipeWire / PulseAudio) is present:

  - **When present, the selectable devices are only the three PCMs routed through the server** (`default` / `pipewire` /
    `pulse`), and direct-to-hardware ones (`sysdefault`, `hw:`, `plughw:`, `front:`,
    `surround*:`, `iec958:`, `hdmi:` …) never enter the list — ALSA hardware devices are exclusive-semantics,
    and grabbing one strangles all other clients on the server;
  - **The default device opens only PCM `default`**, and if it can't, it reports honestly rather than falling back to raw hardware;
  - With no sound server (a headless bare-ALSA system) behavior is unchanged; a direct connection has no
    "strangle others" problem.

  A leftover `audio_device = "Default Audio Device"` in the config won't stop the program: if not found, it logs a line and falls back to the system default (the UI shows the actually opened device).

- **Exit froze when the audio thread was stuck, and the device was never released**. The final `join` on the audio thread was changed to a
  **bounded wait** (3 seconds): normally it exits within one polling cycle (200 ms); if it's stuck in a driver-level blocking operation,
  giving up after the wait — the process exits as usual, the kernel reclaims all fds it holds, and the device is released immediately. `panic` (release builds use `panic = "abort"`) and being `kill -9`'d
  are likewise reclaimed by the kernel, needing no extra handling.

## [0.4.0] - 2026-09-26

### Fixed

- **Resident memory grew to over a hundred MB after continuous playback / frequent track changes, and didn't fall back**. Two things compounded:

  1. The streaming byte buffer **only appends, never reclaims**, so however much a song downloads stays resident (measured 1:1,
     a 64 MiB Hi-Res file means 66.5 MiB);
  2. Track changes **don't cancel** the previous track's download task. Every press of `n` adds another still-running download
     pouring the whole track into memory in the background, with nothing reclaiming it — measured: 5 track changes of 64 MiB each pushed RSS to
     **323.7 MiB**.

  Now the buffer keeps only a 4 MiB tail window around the read pointer, and bytes outside it are read back from the on-disk file via `pread`
  (the download is written to disk anyway, so the data is in the page cache); track change/stop/exit actively cancel the old task.
  Under the same load: single track 66.5 → **12.5 MiB**, 5 track changes 323.7 → **3.0 MiB**,
  and 24 consecutive tracks on the same comparison load stabilize at 12.5 MiB (no longer growing with track count).

- **Playback position suddenly jumped back to the start during streaming playback**. The user sees "it buffers, then plays from the beginning again",
  and the root cause has two layers, both of which had to be fixed:

  1. **Reloading as soon as the stream finished**. When the background download finished it emitted "audio is on disk, ready to play",
     and that event's handler called `audio.load()` once — but by then the song **was already playing**, and
     reloading reset the position to 0. This finalization now goes through a separate event that only clears progress, prefetches, and reclaims cache,
     never touching the player.
  2. **A broken stream was treated as finished**. rodio's decoder swallows any read error as EOF, so "data didn't keep up"
     and "this song finished" look identical at the playback layer; treating it as "finished" triggers a track change, and under repeat-one
     (or a single-track queue) that means playing from 0 again. Now the engine classifies by the stream's real state:
     cancelled (silent), download failed (error, no track change), read timeout (keep the position and resume once), finished
     (then change track).

  Also, the position must be recorded **before the track ends**: once the source ends, `Player::get_pos()` reports 0, and
  reading it on that frame erases the position (the user sees the progress bar suddenly jump back to 00:00). After a broken stream or download failure,
  pressing Space continues from where it stopped, rather than from the start.

- **An interrupted download left "half a song" in the cache**. Streaming download previously wrote the final cache filename directly, and on failure or
  cancellation it left a "looks complete, actually only the first few MB" file; cache lookup judges only by "is the file there",
  so the next play would hit it and end inexplicably in the middle. Worse, it would **restart repeatedly**: the file plays out →
  judged finished → repeat-one from the start → plays out again — the user actually hears a loop every ten-odd seconds.
  Now it writes `.part` first and **renames only when complete**, deleting it on failure/cancel; cache reclamation and "clear cache" also skip
  `.part` (it's a half-finished product, not a cache).

### Changed

- **When opening a playlist, the first screen now fetches from the** last **page**. Previously it always fetched page 1 regardless of how the list displayed,
  while the default display is descending (`o` toggles), so what truly appears on top is the **last page**'s content —
  so the user saw "a flash of mid-playlist songs, then a few seconds later the whole table arrives and the screen flips entirely".
  The measured feedback was "every time I enter a playlist, the first thing I see isn't the first song in my head". Now in descending display
  the first screen fetches the last page directly, so entering shows the head of the final list; in ascending display it still fetches page 1.

  Also fixed a related criterion error: "this page isn't full ⇒ this is everything" **holds only for page 1**.
  The last page not being full is normal (414 songs = 13×30 + 24, so the last page has 24), and copying the old criterion would make "fill the whole table"
  never happen, so the playlist would show only those 24 forever. When the song count is unavailable (some interfaces don't give it) it still falls back to page 1.

### Docs

- Added **[docs/MAINTENANCE.md](docs/MAINTENANCE.md)**: module map, data flow, risk points and
  known pitfalls, and how to run verification after changes. `docs/DESIGN.md`'s "streaming playback" section gained three design notes on window reclamation,
  stream classification, and the `.part` rename.

## [0.3.10] - 2026-09-25

### Fixed

- **The artist list showed only the first group; the whole A–Z table was dropped**. `/artist/lists` returns grouped by initial letter
  (`data.info` is 28 `{title, singer: [...]}`: hot + A–Z + `#`), while the parser treated the group wrapper
  as the artist itself — every group failed to parse, and it finally fell into the fallback scan hitting only the first group, leaving only the hot 60
  artists in the UI. Measured: there should be **1054**. The online log therefore spammed 19 "response matched no candidate key……
  please check the field layout" warnings — that warning itself was shouting "the layout isn't fully recognized", it just had no one going back to check.

- **Clicking into an artist gave zero songs**. `/artist/audios`'s `data` **is itself the song array** (playlists are
  `data.songs`, rankings are `data.songlist`, and only this one is a bare array), while `extract_songs` only looked for named keys
  inside `data`; the array itself never matched, so it always returned empty. After the fix the artist page lists normally.

  Both were found by checking, field by field against real responses, "the candidate keys the parser reads vs. the keys actually present in the response",
  the same shape as the earlier NetEase `ar`/`al`/`dt` case. Each got a unit test written with real JSON.

- **When another service ran on the API port, the error was unreadable**. When the server returned 2xx but not JSON, it previously threw
  serde's raw message `expected value at line 1 column 1`, giving no clue what to check. It's now a separate
  `AppError::NonJsonBody`: it includes the start of the response body (whether it's a web page, an empty response, or a piece of plain text, all obvious at a glance)
  and directly suggests "check that source's `api_base`". Measured hitting this: the config's `[sources.kugou] api_base`
  pointed to 3000, while another web service ran on that port.

- **The release self-sufficiency check falsely reported "missing script"**. In `tar -tzf … | grep -q X`, `grep -q` exits on the first match,
  while tar writes to stdout as it parses, dying from SIGPIPE (exit code 141); `set -o pipefail`
  then judged the whole pipeline as failed — the package was fine but reported missing. Changed to write the listing to a file first and check item by item.

- **The release action list didn't change with `--no-aur`**. Running with `--no-aur` still listed "update AUR" and "push AUR",
  then did nothing. This list's purpose is to let a person confirm "what will happen" before pressing Enter; listing without doing is
  lying, so it's now assembled by switch and states the skip explicitly.

### Added

- **`scripts/release` — release in one command**. Stage one (local, repeatable): prechecks →
  reclaim intermediates → clippy + tests → build → package; then **stop** and list every external action stage two will run,
  waiting for confirmation. Stage two (irreversible): push main → tag → push tag → update and verify
  AUR → create Release → download back and verify → push AUR. Network steps have backoff retries (measured: `git push` reported
  `TLS unexpected eof`), and Release creation is idempotent. When the AUR account is unavailable, `--no-aur` skips the whole thing.

- **`scripts/clean` — reclaim build intermediates**. One release leaves things in three places — `target/package/`, historical tarballs in `dist/`,
  and the AUR repo's `src/` and `pkg/` — measured 10+ MiB per round. By default it clears only clearly regenerable intermediates, **not**
  `target/` as a whole (which would destroy incremental compilation) or the runtime song disk cache
  (user data, and it has its own eviction logic); to clear those, add `--all` / `--cache` explicitly.

### Docs

- Added **`docs/RELEASE.md`**: version and tag rules, what a release artifact should contain, target repositories,
  each step's execution order and dependencies, how to recover after failure, and the trigger timing and scope of the two kinds of cleanup logic.
  "Why the AUR update must come after pushing the tag" is written out separately — that is the only un-reorderable dependency in the whole flow.
- `CONTRIBUTING.md`'s release section now points to the script and the spec. The original manual steps were exactly what the script now does, and
  keeping two copies necessarily drifts.
- Corrected **a false crates.io claim**: the 0.3.9 entry and GitHub release notes said "published to crates.io",
  which was false — `cargo install` only installs the main program, carrying neither those three scripts nor the interface service, a "half"
  install path. Removed from CHANGELOG and the release notes (the original text in the tag can't be changed; this version supersedes it).
- README gained the "prebuilt package in Releases" install path — it works today and was previously only in
  `docs/INSTALL.md`, invisible on the face.
- The version number in `docs/INSTALL.md`'s prebuilt-package example follows to 0.3.10.

## [0.3.9] - 2026-09-25

### Docs

- `CONTRIBUTING.md` gains a **"Release"** section, writing today's several back-to-back releases' pitfalls into steps:
  tool scripts must be committed **before** tagging, AUR's `pkgver`/`sha256sums` follow along and
  `.SRCINFO` must be regenerated, the upstream commit is pinned in **two places**, and **after releasing you must download back and verify**
  (the 0.3.7 tarball missed scripts because only the local directory was checked).
- Fixed two outdated spots in `CONTRIBUTING.md`: "release about 5.4 MiB, resident about 13 MiB" → measured
  7.0 MiB / 14–17 MiB; and a reference to README's "measured corrections" table — that table is actually in
  `docs/DESIGN.md`'s "interface adaptation", and this reference was broken **before** the README was split.
- "Parsing upstream responses must be tolerant" gains a point: **one upstream may have multiple field layouts**, don't recognize only
  the one seen while debugging (NetEase's `artists`/`album`/`duration` vs. `ar`/`al`/`dt`);
  each must be pinned by a unit test.
- Fixed a **link that breaks after installation** in `docs/LICENSES.md`: the docs are installed under
  `/usr/share/doc/kugou-tui/docs/`, while the license, per Arch convention, is under `/usr/share/licenses/`,
  so `../LICENSE` necessarily fails. Changed to stating the path, readable in both cases.

## [0.3.8] - 2026-09-25

### Fixed

- **`kugou-tui-install-api` installed a pile of unneeded dev dependencies**. It ran `npm install`
  (full), while `kugou-api`'s automatic top-up used `npm install --omit=dev` — the two were inconsistent.

  The service runs as `node app.js`, and upstream's 10 devDependencies are all dev tools
  (`nodemon` / `typescript` / `pkg` / `prettier` / `ts-node` / `tsdown` / `@types/*`),
  none of which are used. Measured on the same machine: full install **311** packages, production-only **120**.
  The difference is half pure waiting and wasted disk. Both places now use `--omit=dev`.

### Docs

- `docs/INSTALL.md` gained **"Path 3: prebuilt binaries"**. Previously there were only "build from source" and
  "AUR", leaving non-Arch users with no explanation after getting the Release tarball.
  Now it explains extracting, symlinking the binary and three scripts into `~/.local/bin` (`scripts/kugou-tui` has the same
  name as the binary, so rename when linking), and the `kugou-tui-install-api kugou` step.

> Also, from this version on the Release tarball **contains three scripts and all docs at once** (previously only the
> binary and README). Non-Arch users can set up the interface service right after extracting, without going back to the repo to clone.

## [0.3.7] - 2026-09-25

### Added

- **Support for a "package-provided interface service"**. Prepared for the AUR package's "works right after install": the package installs the interface service together with
  its production dependencies into `/usr/share/kugou-tui/api/<source>` (read-only), and the three scripts now prefer it,
  falling back to the one cloned under `~` only if absent — `kugou-api-install` starts it directly when it finds the system directory,
  **skipping clone and npm install**.

  The reason the service can go into a package: it **writes no files into its own directory** (no `writeFile` /
  `mkdirSync` in the source), and measured, after `chmod -R a-w` on the whole directory it still starts and returns real data.
  So `/usr` stays read-only, unpolluted by npm, with no resident process needed.

  The path can be overridden with `KUGOU_API_SYSTEM_ROOT` (for non-`/usr` prefixes in packaging and tests).

### Fixed

- **The launcher exited silently on a fresh install**. `scripts/kugou-tui` enabled `set -o pipefail`, while reading the config
  used `sed -n ... | head -n 1`; **on a fresh install the config file doesn't exist yet**, so `sed` exited non-zero →
  the pipeline was judged failed → `set -e` made the whole script exit silently (exit code 2, not one message).
  The user's first run was black, and this path is exactly the one "works right after install" must take.

  Now it checks `[ -r ]` for readability before reading. Also merged the `sources.active` extraction, repeated **three times**, into
  one `read_active_source()`.

## [0.3.6] - 2026-09-25

### Fixed

- **The launcher script couldn't find the client binary after being installed to a system directory**. Both `scripts/kugou-tui` and
  `scripts/kugou-api` located the binary as "script directory `/../target/release/kugou-tui`"
  — symlinking into `~/.local/bin` was fine (`readlink -f` resolves back to the repo), but when the script is **copied** to
  `/usr/bin`, that path becomes `/usr/target/...`, which necessarily doesn't exist. This is a pitfall distro packaging always hits
  (found while preparing the AUR package: after install, `kugou-tui` directly reported "executable not found").

  Both now have a `PATH` fallback: the repo's build output first, and the `kugou-tui` on `PATH` if not found. `KUGOU_TUI_BIN` / `KUGOU_API_BIN` can still be set explicitly; the error
  when neither is found now also clarifies "running from source requires building first" and "which variable to set if installed from a package".

## [0.3.5] - 2026-09-25

### Fixed

- **Opening songs inside a NetEase playlist always 404'd**. The playlist list displayed fine, but clicking in reported
  `interface /playlist/track/all/new returned status 404`.

  The reason is that the playlist "first screen" path **bypassed source dispatch**: to "give one page so the UI has content immediately",
  it called Kugou's paging methods directly on `ApiClient` (`user_playlist_tracks` /
  `playlist_tracks`, with parameters `listid` + `page` + `pagesize`, endpoint
  `/playlist/track/all/new`) — that's Kugou's endpoint, which the NetEase service doesn't have at all. And the background fill-in
  went through dispatch (`active_source.*_tracks_all`), except the first screen's failure `return`ed directly,
  **never reaching the fill-in**, so the symptom was "always 404".

  Now paging also goes through dispatch (adding `SourceKind::playlist_tracks_page` and `PlaylistRef`,
  distinguishing Kugou's two endpoint sets for "own playlist / public playlist"; for NetEase both are the same endpoint).

  Also merged the two duplicated paging implementations into one `playlist_tracks_all`, and removed the `SourceKind::stamp` that existed only to "stamp the source for the first screen's fill-in" — after the first screen goes through dispatch, stamping is handled uniformly by the dispatch layer,
  and that patch had no reason to exist.

- **Songs in NetEase playlist and artist pages had no artist, no album, and showed `00:00` duration**. The search page was
  fine, so it wasn't easy to notice.

  NetEase uses **two field-name sets** for the same song: `/search` gives `artists` / `album` / `duration`,
  while `/playlist/track/all` (playlists, rankings) and `/artists` (artist top songs) give `ar` / `al` / `dt`.
  The parser recognized only the former. Now both are recognized, with unit tests pinning both layouts.

- **NetEase cloud playlists always reported "not logged in"**. After QR login the UI showed "login successful", but the playlist page
  kept saying not logged in — the two sides disagreed.

  The root cause is the credential format the server issues: `NeteaseCloudMusicApi` (api-enhanced)'s
  `server.js` splits the `Cookie` header with `/;\s+|(?<!\s)\s+$/g`, **recognizing only the "semicolon + space"**
  separator; while the `cookie` field it returns on a successful `/login/qr/check` is a **whole
  `Set-Cookie`** (with `Max-Age` / `Expires` / `Path` attributes, multiple cookies joined by `;;`).
  `;;` isn't split, so `Path=/openapi/clientlog;;MUSIC_U=00CC…` is treated as
  **one** `k=v`, with key `Path`, and `MUSIC_U` never entering `req.cookies`.

  That is: this credential string was broken from the moment it was stored in the config file, and the UI's "logged in" was a lie.

  Now it's normalized before storing (`util::normalize_cookie_header`: drop attribute segments, drop empty segments,
  reconnect with `"; "`, last-wins for duplicates), **and passes through once more when assembling the request header** — the latter lets existing broken configs
  self-heal without re-scanning the QR code.

- **NetEase requests carried Kugou's `dfid`**. `dfid` is Kugou's proprietary device fingerprint (it splices dfid into
  the cookie for upstream risk control), and NetEase has no such mechanism. Previously it was spliced in unconditionally, sending a
  foreign device identifier to it — useless, and confusing about whose credential string this actually is. Now it's decided by
  `SourceKind::uses_device_fingerprint()`, sent only to Kugou.

- **NetEase's four write interfaces went through a retrying channel** (add song / remove song / create playlist / delete playlist), violating
  "writes are not retried": if deleting a song actually succeeded the first time and only the response was lost, resending gives
  "song doesn't exist" — the user sees a failure message while the song is actually already deleted. Changed to `*_mutating` variants.

- **NetEase popped "failed to fetch user profile" after login**. Fetching the profile went through Kugou's endpoint (without uid),
  and NetEase returned `{"code":400,"message":"参数错误"}`. Now it dispatches by source: NetEase goes through
  `/user/detail?uid=` (nickname, avatar, level). Membership has no corresponding endpoint on NetEase
  (`/user/vip/detail` is a 404), skipped via the new `Capability::vip`, no longer calling a dead interface.

### Docs

- **README split into a face version** (295 → 129 lines): keeping only a one-line slogan, three core selling points,
  the shortest install path, a condensed feature table, the comparison with cmus / mpd + ncmpcpp, and an index pointing to docs/.
  The streaming-playback principle, requirements, launcher scripts and environment variables, third-party license analysis, and full disclaimer moved into `docs/DESIGN.md`, `docs/INSTALL.md`, `docs/LICENSES.md`, `docs/DISCLAIMER.md` respectively.
- Added `docs/INSTALL.md`: requirements, install paths, API service deployment (one-shot + manual),
  launcher scripts and two environment-variable tables, the fish startup function without a resident service, and what to do after installing.
- Added `docs/LICENSES.md`, `docs/DISCLAIMER.md`.
- `docs/USER_GUIDE.md` gained a full "features" table, and the NetEase section now states the cookie's format requirement
  (when filling in manually, write `k=v; k=v`, not pasting a whole `Set-Cookie`).
- **Corrected a wrong assertion about memory in `docs/DESIGN.md`**. The original said "streaming playback costs no extra memory,
  writing into a ring buffer without assembling the whole song in memory", while `StreamingBuffer` is actually an **append-only, never-reclaiming**
  byte buffer (supporting `Seek` requires keeping already-read bytes). Measured: pushing 64 MiB into the buffer raises RSS by 64 MiB
  (1:1), so playing tens of MB of Hi-Res makes memory grow linearly with track size. Now stated honestly in the doc,
  and the README's memory-comparison footnote gained a boundary note.

### Other

- `scripts/license-stats.py` now uses `cargo metadata` to get the `license` field. It previously read
  `$CARGO_HOME/registry/src/<name>-<version>/Cargo.toml` directly, so the "unreadable" count drifted with the local
  cache — the same `Cargo.lock` once yielded 26 unreadable and once 102, numbers impossible to verify.
  Now the result depends only on `Cargo.lock`: of 468 dependencies, 455 permissive, 13 `MPL-2.0`, 0 unreadable.

## [0.3.4] - 2026-09-25

### Added

- **Selectable output device**: the settings page's last item "output device" shows **the card actually opened**, switchable between
  "system default" and the enumerated devices (written to the config's `audio_device`).
  This is prepared for a very common failure: the progress bar moves and the state is "playing", but there's no sound
  — the sound was sent to another card. Now it's visible without digging into system config. Switching devices pauses the current
  track briefly, then resumes at the original position once the new device is ready; **if the new device can't open, it keeps using the old one**, with one notice,
  not muting playback.

  The candidate list is filtered twice: ALSA reports all the PCMs it defines as devices (measured 52 on this machine, mostly
  `lavrate` / `samplerate` / `jack` / `oss` plugins), so it keeps only those "able to give a default output configuration",
  then excludes `null` (Discard all samples — it opens and "plays normally", just discarding all samples, another cause of
  "playing but no sound"), and finally deduplicates by name.

  "System default" shows the `default` card's self-described name (like
  `Default ALSA Output (currently PipeWire Media Server)`) rather than cpal's hardcoded
  "Default Audio Device" — the latter doesn't show who the sound was actually handed to.

### Fixed

- **"Playing but no sound" can now prove its innocence**: previously there was no way to start on this — the device opened fine,
  decoding was fine, the state was playing, and there was not one error message. Now the UI directly shows the actual device name, so
  troubleshooting takes one look. The measured root cause is on the system side: `/etc/asound.conf` hardcoded ALSA's `default` to
  `card 2`, and that card was no longer the USB sound card the user listened on (card numbers change, so hardcoding a number will eventually bite),
  so the sound went to an onboard port with nothing plugged in. And because it used `type hw` (exclusive), it even bypassed PipeWire —
  the program didn't appear in `pactl list sink-inputs` at all.

## [0.3.3] - 2026-09-24

### Added

- **System tray**: registers as `org.kde.StatusNotifierItem` at startup, so status bars like Quickshell / waybar /
  KDE show an icon, hovering shows the current track, and **right-click pops a menu** (play / pause,
  previous, next, via `com.canonical.dbusmenu`). **Self-adapting**, with three degradation levels all skipped silently,
  not affecting playback: no graphical session (neither `WAYLAND_DISPLAY` nor `DISPLAY`) → not connecting to
  D-Bus at all; no session bus → skip; the status bar doesn't provide `org.kde.StatusNotifierWatcher` →
  give up after the registration call fails. Disable with `--no-tray` or `tray = false` in the config
  (**takes effect on restart**). The icon is embedded (source `assets/tray.svg`, scaled at runtime to 22 / 64
  sizes), independent of the system icon theme.
- **The tray context menu** must give the `Menu` property a **real object path**. In the SNI spec `/` means
  "no menu", but measured, Quickshell skips right-click entirely based on it (`hasMenu` is false),
  so "clicking does nothing" — so this registers `/StatusNotifierItem/menu` and implements
  `com.canonical.dbusmenu`. The left-click `activate()` is still left empty as the user requested.
- **"Minimize / show window" in the tray menu**: collapses the TUI out of the tiling layout (music keeps playing),
  and clicking again restores it. A terminal program can't minimize its own window (xdg-shell has no such request), so it can only borrow the
  compositor's IPC — going through niri's `toggle-window-minimized`. **How to find its own window**
  is the only hard part here: the `pid` niri gives is the **terminal emulator**'s (kitty etc.), which won't match, so
  at startup it sets the terminal title to `kugou-tui` with OSC 0, and afterwards matches the window back by title **exactly**
  (`contains` can't be used: measured, some browser tabs have titles containing `kugou-tui`, which would minimize the browser
  instead). This item appears in the menu only when `NIRI_SOCKET` is detected — if not niri, the whole item
  doesn't appear, rather than being grayed out.

### Fixed

- **The TUI screen polluted by audio-backend errors (garbage)**: libjack / libasound **write errors directly to fd 2**,
  e.g. `jack server is not running or cannot be started`,
  `JackShmReadWritePtr::~JackShmReadWritePtr - Init not done for -1, skipping unlock`,
  `ALSA lib pcm_oss.c:404 ... Cannot open device /dev/dsp`. When the TUI is on the alternate screen,
  these characters are printed directly into the interface ratatui drew, and **incremental redraw only writes "cells whose content changed"**
  — the third-party characters on screen aren't in any buffer and are never overwritten, so they linger as garbage.
  **The narrower the window, the more obvious**: an error line wraps into multiple rows, while maximized it's one row and nearly invisible.
  Now, before entering the TUI, stderr is redirected to the log file (`logger::redirect_stderr_to_log`,
  using `dup2`): the screen is clean, and the errors remain in the log — when troubleshooting "no sound" they are exactly the key clue.
- **Under the concept-edition source, playing from a cloud playlist only gave previews**: `Song::source` parsing filled the default
  `Kugou`, while `stamp_songs()` covered only 5 `*_tracks_all` methods — the playlist **first screen**, to "get the UI up first",
  called `ApiClient::user_playlist_tracks` directly, bypassing the stamp, so this batch of songs was treated as
  standard-edition songs and stream requests went to the standard-edition port; the two platforms' tokens aren't interchangeable, so the standard edition could only give 60-second previews.
  Now `SourceKind` has a public `stamp()`, and the first screen's result gets one extra stamp.
  (Fix from [@ccchenyulin](https://github.com/ccchenyulin))
- **Custom letter keys stole input-box keys**: the `[keymap]` custom keybinding table intercepted **before** the mode check,
  so after binding `seek_forward` to `l`, you couldn't type `l` in the search box. Now in input mode
  unmodified character keys go to the input box first (`ctrl+` / `alt+` combinations still go through the custom table). Two related fixes:
  the new-playlist dialog now uses `TextInput` (with its own cursor / Delete / Home / End) and shows a real cursor;
  and prompts like "press R to retry" in the help panel and UI are rewritten by `[keymap]` before display.
  (Fix from [@ccchenyulin](https://github.com/ccchenyulin))

### Docs

- README's first-screen interface diagram changed from hand-drawn ASCII to a **real terminal screenshot** (`assets/screenshot-0.3.3.jpg`,
  950×1021, JPEG compressed to 175 KB).

## [0.3.2] - 2026-09-24

### Fixed

- **`f` couldn't be rebound via `[keymap]`**: in `parse_key`, the F1–F12 branch checks "length ≤ 3 and
  starts with `f`", which `f` also satisfies, so `f[1..]` is an empty string, parsing fails, and the whole thing returns `None`.
  The result: **only `f`** (artist region filter) couldn't be bound in the config, while other single letters could. Found while writing the "keys in the help panel are really bound" test
  — the test hung on it the first time it ran.
- `scripts/kugou-api-install` used `git clone --depth 1` to fetch the default branch's tip, while the README
  required pinning to a verified commit; the two disagreed, and a shallow clone can't `checkout` that SHA at all.
  Now it uses `git fetch --depth 1 origin <sha>` to fetch just that one commit (measured: completes within 1 second,
  `.git` only 416K), and **checks an already-installed copy too** — when HEAD doesn't match the pinned commit it gives a
  hint and the switch command, without switching on its own.
- `scripts/kugou-api-install`'s `start_kugou` didn't pass the install directory to `kugou-api`.
  `KUGOU_API_ROOT` and `KUGOU_API_DIR` are two variable sets, so with a non-default ROOT it would "install in one place,
  start from another".
- `kugou-api-install`'s usage comment said `qqmusic`, while the implementation supports only kugou and netease
  — that example would only yield "unknown source".

### Added

- **`scripts/kugou-api` gains restart, PID management, and a startup precheck**: the precheck fills in `node_modules`
  (automatic `npm install --omit=dev`) and the client binary (automatic `cargo build --release`);
  parameters are resolved by "environment variables > config file > defaults"; after a background start it prints the PID, log path, and access
  address; a new `restart`; and port-in-use is distinguished from "already running". See the script header comment for details.

### Docs

- **`[keymap]` (custom keybindings) had not one word in the docs before**. CONFIGURATION.md gains
  a "Custom keybindings" section: section syntax, key-name rules, 59 available action names (grouped by use, checked character by character against
  `action_from_name`), and the log text for each of the three kinds of illegal entries.
- Added 5 keybindings that "can be pressed but are written nowhere": `E` / `J` / `K` (the source page's set-default and
  priority-adjust — the only entry for adjusting source priority) into the help panel, and `Home` / `End` (`g` / `G`'s
  aliases) into KEYBINDINGS.md.
- README's install step 1 now lists `scripts/kugou-api-install` as the recommended path, and points out that
  KuGouMusicApi is an **independent repository** (no submodule, no vendor) that must be present locally; CONTRIBUTING's manual clone also gained commit pinning.

## [0.3.1] - 2026-09-24

### Fixed

- **The visualizer's empty state called "stopped" "paused"**: it judged only "is there a current track", so
  `Stopped` also showed "paused — press Space to continue". But the two differ in what Space does: stop is
  **play from the start**, pause is **continue**. The next second after pressing, the UI lies. Now they're stated separately.
- The sidebar's "clear cache" keybinding and directory name were stuck together into one word (`C清理 kugou-tui`). Changed to the `[C]`
  bracket style, consistent with the status bar's "[?] help [q] quit".

## [0.3.0] - 2026-09-24

> **Behavior change**: startup no longer prints "connected" — at that moment not one request has been sent, and it would say the same
> even if every interface were down. Now it states only the config and login state, and connectivity is driven by the result of real requests (the sidebar's
> "connection" block title shows connected / disconnected / unverified).
>
> Also, when a list fails to load the panel shows "load failed: … · press R to retry", rather than staying on
> "loading…" forever.

### Added

- **Automatic retry on transient network failures**. This machine goes through a fake-IP proxy with large latency swings: one connection refused,
> or a response body cut off mid-read, fails the request directly, and such failures **succeed on a resend at another moment**.
  The costliest is the stream-resolution path — `song_stream_url` tries multiple candidates
  (hash, quality) one by one, and a candidate skipped due to one hiccup means after all hiccup, the user sees
  "no playable address available (may require VIP or be delisted)", a conclusion unrelated to the real cause.
  Now read interfaces retry automatically: **3 attempts total (first + 2 retries), 300 ms → 900 ms apart**.
  - **Retried**: transport-layer breaks at connect or body read (connection refused / reset / truncated body),
    and HTTP 408 / 429 / 5xx.
  - **Not retried**: **timeouts** (already waited the full 15 seconds, meaning the peer is stuck rather than hiccuping; two more rounds
    trade the user's time for a most likely identical result); business error codes (the service responded, resending gets the same
    answer); other 4xx; 200 with non-JSON (a content problem, not a transport problem).
- **Write interfaces explicitly don't retry** (`get_json_mutating` / `get_json_uncached_mutating`).
  Retrying presupposes "resending doesn't change the result", which writes don't satisfy: if `/playlist/del` actually deleted
  successfully the first time and only the response was lost in transit, resending gets "playlist doesn't exist" — the user sees a failure message,
  while the playlist is actually gone. Favorite / remove / delete playlist / create playlist / claim VIP, six places, use the non-retrying
  variants; other read interfaces keep automatic retry.
- **The help panel is scrollable** (`?` to open): `CHEATSHEET` has 38 entries, while a 34-row terminal fits only
  26; and the panel is modal, swallowing all keys, so the last 12 — Space / `n`·`p` /
  `←`·`→` / `+`·`-` / `m` / `r` / `l` / `[`·`]` / `W`, that is **the entire playback control block**
  — were never visible at common sizes. Now it supports `j`/`k`, `↑`/`↓`, `PgUp`/`PgDn`, `g`/`G`,
  with the bottom always showing "entries N-M / 38 total", hidden when the content fits.

### Fixed

- **Lists stayed on "loading…" forever when the interface was down**: `loading` was originally cleared only on the success path
  (`replace()`), and 11 places set it with no failure exit. So once a request failed, the panel kept spinning — the status bar reported
  an error, the panel kept spinning, and the user neither knew it failed nor what to press. Now a failure clears "loading" and puts the reason
  on the panel ("load failed: … · press R to retry"). Affected: playlist plaza, artists, rankings, cloud playlists, search, and the song lists of playlists/artists/rankings.
- **Startup unconditionally printed "connected"**: this was emitted on the startup path, at which point not one request had been sent,
  and it would say the same even if every interface were down — the user sees "connected" and rules out a network problem,
  then looks elsewhere. Now startup states only the config and login state
  (`source kugou concept edition · 127.0.0.1:3001 (logged in)`), and connectivity is driven by the result of real requests:
  the sidebar's "connection" block title shows `connected` / `disconnected` / `unverified`, and only on the first successful
  request does it change to "connected". Note that business error codes (login required, page out of range…) count as **connected** —
  they actually prove the service is up; only connection refused / timeout count as unreachable.
- **"My profile" only wrote a log when it couldn't fetch the profile**: the home page would stay on "loading…" forever, and the user couldn't tell
  whether it failed or was slow. Now it shows the failure reason, with a clickable retry entry (anywhere in the account area).
- The song-list titles for playlists / artists / rankings no longer carry "(loading…)": the panel body already shows
  "loading…", so a second one on the title is redundant; and on load failure that suffix would stay on the title lying.
- **The help panel's column widths now compute from the data**: they were hardcoded `14 / Min(20) / 6`, so the "function" column
  ate all remaining width, pushing "category" to the far right of the dialog with twenty-odd blank columns in between; and the "key" column was hardcoded
  14 while the longest key name (`Tab / S-Tab`) is only 11 columns. Now both columns compute from the longest entry, so the gap is
  gone; and when too narrow for three columns the "category" column yields entirely (it's a group label that doesn't affect "what this key does"),
  rather than squeezing "key" to 0 width — on a 30-column terminal previously only the "function" column was visible.
- `right_click_at`'s doc comment said "**no context menu**", while the body called
  `open_context_menu()`. The comment now matches the implementation (right-click = keyboard `;`).
- **A lyric fetch failure no longer says "no lyrics"**. Previously a failure stuffed in an empty lyric, and the panel rendered
  "no lyrics" — that asserts on the song's behalf "**it has no lyrics**", sending the user to look elsewhere. Now it goes through
  the separate `Loaded::LyricFailed`, and the panel shows "load failed: <reason>". Empty lyrics (request succeeded but content is empty) still
  say "no lyrics" as usual, and the two are no longer conflated. Related: lyric requests also get automatic retry (measured: injecting 503 makes it retry twice before giving up).
  Not going through `Loaded::Failed` is deliberate — that path writes an error to the status bar, and a lyric failure doesn't affect playback,
  so flashing one on every track change is too noisy.

## [0.2.0] - 2026-09-22

> **Breaking change**: removed the "lyrics" and "cover" tabs, and reassigned number keys `8` / `9` to sources and settings;
> the targets of `1`–`7` and `0` are unchanged. See the "Removed" section below.

### Added

- **Settings page** (open with `,`, or click the sidebar's last item): theme, quality, playback
  mode, refresh interval, lyric offset, page size, cache limit, 16-color mode, lyric panel, side navigation,
  download directory, and cover fill mode. ↑↓ to select, ←→ / Enter to change, mouse click to select, click again to change.
  Saved to `config.toml` immediately, without waiting for exit.
- **6 themes**: cool blue / graphite / sunset / forest / neon / dark purple (`config.theme`).
  Each has a truecolor and a 16-color version; the 16-color version changes only the primary color and selection background, while semantic colors
  (success / warning / error) don't vary by theme.
- **Track download** (`W`): saves the currently playing song to `config.download_dir`
  (default `~/Music`, changeable on the settings page). Filename `<artist> - <title>.<ext>`,
  with the extension inferred from the URL's last segment; existing files aren't overwritten.
- **Per-character lyrics (karaoke effect)**: parses KRC's per-character timestamps, coloring the current line by
  sung / **singing** / unsung, lighting up as it's sung. When per-character information is unavailable (plain LRC or this line has no
  per-character markers) it falls back to whole-line highlight.
  Note KRC's format is `<offset,duration,0>char` — **the marker comes before the character** (not `char<...>`).
- **Chunked concurrent download**: if the direct link from `/song/url` supports Range, it downloads in up to 4 concurrent
  chunks; if unsupported or the file is too small, it falls back to a single connection. Measured: a 128kbps track 1.41s → 1.10s.
- **Prefetch the next track during playback**: after the current one starts, the background downloads the queue's next track into the cache, so track changes are instant.
- **`cover_fill` config item**: how the big cover on the home page fills its area, choosing from `crop` (default,
  center crop), `stretch` (stretch to fill), `fit` (full display, letterboxed). The cover area is
  "how many columns × how many rows", which converted to pixels is almost never square, while album art usually is —
  fill / no distortion / no cropping, only two of the three, and this switch lets you pick.
  See [docs/CONFIGURATION.md](docs/CONFIGURATION.md).
- **Daily concept-edition VIP claim**: Kugou's concept edition has a built-in "claim one day of VIP per day" mechanism, now made
  automatic — attempted after a successful login, on switching to the concept-edition source, and at startup, **once per day**
  (the date is recorded in the session). The flow follows MoeKoeMusic: first claim one day (`/youth/day/vip`),
  then after 500 ms upgrade to "unlimited-listening VIP" (`/youth/day/vip/upgrade`).
  "My profile" gains a status line (`claim today's VIP · press V` / `claiming…` / `today's VIP already claimed`),
  triggered by the `V` shortcut or by **clicking that line**. **Shown only under the concept-edition source** — this is a concept-edition
  exclusive interface that standard-edition accounts can't call, and a dead entry point is worse than none.
  The interface is rate-limited; on failure the prompt puts "repeated failures, claim on mobile" first (the status bar truncates
  by width, so writing it later makes it invisible).

### Changed

- **Removed the "lyrics" and "cover" tabs**: the home page (split left/right) already has the cover and lyrics,
  and list pages have a lyric panel in the bottom right, so these two were the third and fourth duplicates. The main area's space is returned to the home page and lists.
- **The home page moved to the sidebar's first item**: previously the "now playing" group came after "discover / mine",
  so the home page as the default landing point took seven rows down to see. Now the group is at the top.
- **Number keys 8 / 9 reassigned to "sources" and "settings"**: after removing pages, if the later tabs shifted up,
  `0` (visualizer) would be pushed to `8`, and sources and settings would move too, wrecking muscle memory. Now
  `visualizer` stays at index 9 (still the `0` key), and the freed 8 and 9 go to the previously unreachable
  sources and settings: **1–7 and 0 are all untouched**.
- **Per-character lyrics changed to imitate Apple Music**: previously each character had only three levels, "unsung / singing / sung",
  advancing in hard jumps. Now each character interpolates between the base and accent colors by **its own progress**
  (`LyricWord::progress_at`), with boundary characters taking the transition between the two colors, looking like a gradient sweep. Three supporting changes:
  - Non-current lines **dim linearly** with distance from the current line (new theme color `lyric_far`), so only the current line stands out;
  - Removed the "singing" underline — it jumps between characters as the per-character progress advances, more distracting than not having it;
  - When lyrics are actually on screen **and** playing, refresh is raised to ~30fps (previously only the visualizer page sped up).
    At 5fps even a fine interpolation is a jump every five cells. When not watching lyrics it costs nothing extra, and lite mode
    (`lite_mode`) still does no speedup at all.
  - 16-color mode has no intermediate levels to interpolate, so `mix` automatically falls back to picking one of the two ends, equivalent to the original three-level discrete behavior.
    To get closer to Apple Music's look you need a truecolor terminal (`basic_color = false`).

- **The visualizer changed to a real spectrum**: it previously drew a 28-cell time-domain volume history, stretched to fill the full width,
  with no gaps between bars, so when moving it was a whole wall of peaks and troughs. Now it FFTs the samples captured on the audio thread and splits into 64 frequency bands
  by **log scale** (40Hz–16kHz), with 1 column per bar + 1 column gap.
- **The wheel scrolls one line at a time** (previously 3 lines).
- **Cover rendering handed to `ratatui-image`**: no longer writing kitty escape sequences to stdout itself,
  but letting the widget write into ratatui's Buffer — not one byte resent when the content is unchanged.
  Depends on `ratatui-image` 11.x, `rust-version` raised to 1.86.
- **README's interface image changed to a real screenshot**: the previous one was hand-drawn, stuck at "5 tabs, no grouping,
  no number keys", long out of sync with the actual UI, yet labeled "actual screen". Now a mock backend
  (playlist plaza + playlist songs + per-character lyrics + a **silent** audio track generated by ffmpeg) drives the program to render
  a real 108×30 screen — the data is synthesized, containing no real music content, and will no longer drift from the code.

### About playback waiting

**Streaming playback is already done**: after resolving the direct link it accumulates 128 KB (~8 seconds of audio) and starts playing, with the rest downloading in the background
while being cached to disk, so the wait is "accumulate the beginning", not "download the whole song".

The remaining wait is determined by the **quality tier**: `high` (Hi-Res) measured 65 MB per track, which at 1.9 MB/s on a single connection
takes 34 seconds — a physical limit of file size. Chunked concurrent download (up to 4 chunks) + prefetching the queue's next track can
bring track changes to instant, and the second play of the same track hits the cache directly. To go faster you can only lower the quality
(`flac` ~30 MB / `320` ~10 MB).

> When the streaming buffer's read pointer reaches a position not yet downloaded, it blocks waiting for data, showing as a brief pause in the sound.

### Fixed

- **`s` / `a` / `d` reported "no song currently selected"**: these actions previously recognized only "focus is in the song list
  column", so when focus was on the **upper part** of a playlist / artist / ranking, or on the **queue page**, no song could be obtained.
  Added `selected_song()` which falls back through "queue selection → current page's song list → queue current → now playing", so
  favoriting (`s`), adding to queue (`a`), and removing from a cloud playlist (`d`) work in more places.
  Also fixed it giving up directly when the queue had no selection.
- **Clicking "sources" in the sidebar did nothing**: the click went through `Tab::from_number`, a number-key mapping
  covering only the first 10 pages, while sources is 11th → got `None`, doing nothing. The mouse is now free of
  the keyboard's ten-key limit.
- **VIP / delisted songs couldn't get a playback address** (several places, fixed one by one):
  - `/privilege/lite` must use **GET + a `hash` parameter**. The server reads the top-level `hash`
    (comma-separated, multiple allowed), not a `resource` array; using POST + `resource` gets
    `error_code 20010 "hash and AlbumAudioID is empty"`, and 20010 was treated by our client
    as "login required", silently failing the whole path — the log's "login required" had nothing to do with login state.
  - **The concept edition must pass `ppage_id=356753938`** (a single one). Without it the server uses the default three-segment value
    `356753938,823673182,967485191`, making delisted songs return `status=3`. The standard edition ignores the client-
    passed value (using its own hardcoded one), so passing it uniformly is safe for both platforms.
  - **Never pass `album_audio_id`**: measured, with it the server **takes the song by it alone and ignores hash** —
    fixing one aid with three different hashes returns the same URL, **silently playing a different song**.
    MoeKoeMusic also passes only hash for locating.
  - **Try candidates one by one; a single failure doesn't abort**: when one combination returns 502, the whole flow no longer fails.
  - Corrected the `status=3` text: it isn't "no copyright / delisted" but "file identifier doesn't match account permissions"; the original text misled users into thinking the song was delisted.
- **Terminal capability detection swallowed the whole keyboard**: `Picker::from_query_stdio()` blocked reading
  stdin after sending a query, and that read had no timeout of its own; when the terminal doesn't respond (tmux, some terminals, some SSH) the thread
  hangs forever in `stdin().read()`, after which all keypresses are eaten by it. Changed to reading only environment variables.
  Also wrote the detection result (protocol type + cell pixel size) into the log: when the cover displays wrong,
  one look tells you whether the graphics protocol wasn't hit at all.
- **The home cover "isn't completely filled"**: the root cause isn't that it isn't scaled up, but that **the box and the image shapes don't match**.
  `ratatui-image`'s three `Resize` modes (`Fit` / `Scale` / `Crop`) **all preserve the aspect ratio** —
  `Scale` isn't "stretch to the area's size", it just "allows upscaling", still proportional. And the cover area's
  pixel ratio (~1.5:1) never matches square cover art, so whichever is chosen leaves black bars.
  Now it crops/scales the image itself by the target area's pixel size (equivalent to CSS `object-fit: cover`),
  then hands it to the protocol renderer, achieving a true fill.
- **Pressing `↑` `↓` / `j` `k` in the sidebar jumped to the wrong page**: movement went through `Tab::ALL`
  (number-key targets), while the sidebar **displays** `Tab::SIDEBAR_ORDER`, and the two orders differ —
  pressing down from "queue" jumped to "lyrics", while the next one on screen clearly says "home". First/last item jumps
  (`g` / `G`) likewise. Now they uniformly follow display order, and the mouse click's hit zone is interpreted by display order too.
- **Pressing `0` did nothing at all**: the keybinding table's branch wrote `'1'..='9'`, omitting `0`.
  The sidebar printed "0 visualizer", the docs said "1–9, 0", and `Tab::number_key()` returned `'0'` as usual — the UI and docs lied together, and no error.
  Now `0`-`9` all work.
- **Two shortcuts written wrong in the docs**: the settings page labeled the lyric panel `y` (actually `l`), and side navigation
  `b` (actually `\`); the help panel and KEYBINDINGS also claimed number keys could "jump to item N in a list",
  but that feature doesn't exist (focus is usually on the song list, and actually implementing it would break the most common "press a number to switch page").
  Corrected to match actual behavior.
- **The right segment of the playback bar was truncated early**: that segment's width was computed by **character count**,
  while truncation was done by **display width** — in "next <Chinese title>" each character of the title occupies two columns, so the computed width was clearly too small, and
  space that clearly existed was truncated to "next WE GO · playing · …". Changed to compute with `display_width`.
  (Spotted only while capturing the real screenshot for the README: that ellipsis was very conspicuous.)

### Docs

The following were **outdated or left behind by feature changes**, corrected to match the current code:

- **"Streaming playback isn't done"**: README, FAQ, and CHANGELOG all still said "download first, then decode,
  not streaming playback". It was in fact already implemented (starts playing after 128 KB). Also added its real limitation:
  the read pointer blocking at an undownloaded position, and that **resuming the last position doesn't go through streaming** (it downloads fully first).
- **Rust version**: README / CONTRIBUTING / one source comment said 1.85, while the actual `rust-version`
  is **1.86** (determined by `ratatui-image` 11.x).
- **Node version**: wrote "16+", while upstream `engines` is actually `>=12`.
- **Platform**: wrote "linux-macos-windows", but `zbus` on Windows requires `async-io`
  while this enables only `tokio`, so Windows doesn't compile. Changed to `linux`, with an explanation in the docs.
- **Memory**: README said 14 MiB, DESIGN/CONFIGURATION said 13.8/16.8 MiB, four places disagreeing.
  Re-measured (VmRSS): empty data 14.2, logged-in home 15.3, playlist page 15.8,
  playing 16.9 MiB.
- **Size and dependency count**: 6.7 MiB → **6.8 MiB**; `Cargo.lock` 468 → **469** packages.
- **Quick-start's "press `2` to open the playlist plaza"**: after the number-key change the playlist is `3`, and `2` is search.
- **Third-party license table**: the numbers were outdated (427/21/13/7), and it claimed "extracted automatically by a script" while the script wasn't
  in the repo at all. Now recomputed as 429 / 13 / 26 per `scripts/license-stats.py`, and **the script actually committed**,
  so it can be regenerated later.
- **Terminal requirement**: wrote "256 colors", but the themes and per-character gradient need truecolor (24-bit) to be complete.

## [0.1.0] - 2026-09-20

First usable version, released at https://github.com/sijin-xb/kugou-tui

### Added

- **MPRIS desktop integration**: registers as `org.mpris.MediaPlayer2.kugou-tui`.
  The status bar / media controls / `playerctl` can control directly and show the cover; supports dragging the progress bar (SetPosition).
  Skipped automatically without D-Bus, not affecting playback.

- **Audio visualization**: the navigation's 6th tab (enter with `6`).
  Data comes from the real peaks the audio thread records while passing samples through — **not a random animation**:
  silence drops to the bottom, drum hits push to the top. With "fast rise, slow fall" easing and peak hold.
- **Lyric translations / transliterations**: parses the `[language:base64]` tag in KRC (undocumented officially),
  attaching to lyrics by line. The request format changed from `fmt=lrc` to `fmt=krc` — translations exist only in KRC.

### Fixed

- **The source no longer falls back**: the top-level `api_base` and `sources.active` are two independently persisted
  fields, unaligned at startup, causing "concept edition before exit, standard edition after restart", needing a manual `v`.
  Unified between `Config::load()` and `merge_cli()` (the command-line `--api-base` can still override a single session).
- **Empty song titles in search results**: `/search`'s title field is `FileName` (capital F/N),
  while the candidate key list had only lowercase `filename`, so the case mismatch made everything show `-`.
- **The visualizer page's arrow keys did nothing at all**: in `move_selection` the visualizer branch was written first,
  ahead of `(_, Focus::Sidebar)`, so that page didn't even respond to sidebar tab switching. Moved to last.
- **Source profile pollution**: `sync_active_source()` used to write the runtime `api_base` back into the source profile,
  so pointing elsewhere temporarily with `--api-base` permanently corrupted that source's address. Changed to syncing only cookie / dfid.

### Changed

- The depended-on KuGouMusicApi is locked to `a5a9801` (upstream is active and interface fields change).
- Corrected an outdated "Kugou ↔ NetEase" comment — the NetEase source was long removed, and it's actually "Kugou ↔ Kugou concept edition".

### Known limitations

- Lyric translations exist only when the server's KRC carries a `[language:]` tag, and some songs don't.
- `/artist/lists` occasionally returns an upstream 502, requiring a manual `R` refresh (retry not yet added).
