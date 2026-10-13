# Maintenance and Troubleshooting Handbook

**When to read this**: when changing the playback/download/cache path; when a user reports
"memory keeps growing", "the progress bar jumps back to the start", "it skips songs while
playing", or "there's half a song in the cache"; or when returning to the project after a long
break and wanting to know where the modules live.

It records **pitfalls and verification methods**, not feature descriptions — for features see
[README](../README.md) and [docs/USER_GUIDE.md](USER_GUIDE.md), for design trade-offs see
[docs/DESIGN.md](DESIGN.md).

---

## 1. Map

### 1.1 Directory

```
src/
├─ main.rs            process entry: parse CLI → assemble App → run main loop → exit
├─ cli.rs             command-line args (precedence: CLI > env vars > config file > defaults)
├─ config.rs          config read/write, paths (Linux: ~/.config/kugou-tui/, ~/.cache/kugou-tui/;
│                     Windows: %APPDATA%\\kugou-tui\\, %LOCALAPPDATA%\\kugou-tui\\)
├─ event.rs           the process-wide single EventBus and the Loaded event enum
├─ keymap.rs          key → semantic Action; the CHEATSHEET / REBINDABLE lists
├─ logger.rs          minimal file logging (`tlog!`); DEBUG needs KUGOU_TUI_DEBUG=1
├─ error.rs           AppError and the "retry or not" criterion
├─ util.rs            time formatting, random numbers, small helpers
├─ bootstrap.rs       auto-bootstrap of the local KuGouMusicApi (Node service); with
│                     `--no-default-features`, bootstrap_disabled.rs takes over (same name
│                     and signature, everything errors)
├─ bootstrap_disabled.rs
├─ mpris.rs           desktop integration: MPRIS (playerctl / DMS, etc.) — **Unix only**, see below
├─ tray.rs            system tray — **Unix only**, same
├─ window.rs          niri window control (minimize / show)
├─ ws.rs              WebSocket service: state / lyric push + remote control (127.0.0.1 only)
├─ api/               API layer
│  ├─ traits.rs       unified audio-source interface (sources implement it, app only sees it)
│  ├─ client.rs       HTTP with retries; AppError classification lives here
│  ├─ catalog.rs      search / playlists / rankings / artists / fetch playback direct URL
│  ├─ cloud.rs        account, playlist sync, VIP
│  ├─ lyric.rs        KRC word-by-word lyric parsing
│  ├─ model.rs        data models such as Song + tolerant field parsing
│  ├─ node.rs         `--api node` fallback: implementation via the local KuGouMusicApi
│  └─ native/         embedded pure-Rust KuGou API (`--api native`, default)
│     ├─ mod.rs       in-process implementation of the 26 routes, aligned to upstream item by item
│     ├─ sign.rs      two signature schemes (Android / Web), signKey / signParamsKey
│     ├─ crypto.rs    RSA / AES / MD5 etc.
│     ├─ device.rs    device fingerprint (calculateMid / getGuid / randomString)
│     ├─ krc.rs       KRC lyric decryption
│     └─ transport.rs construction of outbound requests (URL / params / headers / body)
├─ audio/             audio subsystem (focus of this doc)
│  ├─ engine.rs       playback engine owning a dedicated audio thread
│  ├─ streaming.rs    download-while-playing byte buffer (implements Read + Seek)
│  ├─ download.rs     download: streaming / whole-track to disk
│  ├─ cache.rs        disk cache and capacity reclamation
│  ├─ resample.rs     anti-aliasing resampling of hi-res masters
│  ├─ downmix.rs      multichannel downmix (see §4 item 12)
│  ├─ levels.rs       level capture (ring buffer + atomics)
│  └─ spectrum.rs     FFT spectrum
├─ source/            audio-source abstraction (Kugou / Kugou Concept / NetEase / Soda)
│  ├─ mod.rs          SourceKind, per-source profiles and dispatch
│  ├─ netease.rs      NetEase (via the local NeteaseCloudMusicApi)
│  └─ sodam/          Soda (direct public network + CDP signing page); data plane from crates/libresoda
├─ app/               state machine (main thread only)
│  ├─ mod.rs          App assembly, main loop, exit
│  ├─ state.rs        AppState: the single source of truth for the UI
│  ├─ update.rs       event → state change (dispatch + loading layer + result handling, see §1.8)
│  ├─ navigation.rs   navigation: page switch / focus / selection movement / number keys (split out of update.rs)
│  ├─ playback.rs     playback control: start / track change / progress / volume / mute / lyric offset
│  ├─ search.rs       search: submit and paginated append
│  ├─ cloud.rs        login / source switch / VIP / cloud playlists / account profile
│  ├─ settings.rs     settings page: candidate values + display text + three interactions (move / click / change value)
│  ├─ desktop.rs      MPRIS / system tray / window control / WebSocket snapshot
│  ├─ queue.rs        playback queue and playback modes
│  └─ session.rs      session persistence (where playback last was)
└─ ui/                ratatui rendering (read-only state, no mutation)
   ├─ mod.rs          layout
   ├─ theme.rs        colors (true color / 16-color variants)
   ├─ icons.rs        icons (Nerd Font / ASCII variants)
   ├─ widgets.rs      rendering primitives: list rows, panels, QR codes, etc.
   └─ views/          sidebar, lists, playback bar, lyrics, help, settings, sources, visualization

crates/
└─ libresoda/         data plane for Soda music (separate crate, package name libresoda-fork)
                     search / stream fetch / decryption / playlists / artists / albums / lyrics / CDP QR login,
                     the same library upstream SodaM uses. `tools/qishui-signer/` holds the scripts
                     used by the CDP signing page (the js and html under `security/`)
```

### 1.2 Threads and ownership

```
input thread ──┐
               ├─→ EventBus ─→ main loop(update.rs) ─→ draw
async tasks ───┘                      │
                                      └─→ AppState (main thread only, lock-free)
audio thread(kugou-audio) ───────────→ atomics (position/duration/volume/state) + EventBus
ws thread(ws.rs) ────────────────────→ its own current_thread runtime (see 1.9)
```

Three iron rules:

1. **Only the main thread mutates `AppState`**, so state has no lock. Don't touch `AppState` from
   async tasks.
2. Products of async tasks always **return to the main thread** and are consumed by `update.rs`
   (`bus.emit(Loaded::…)`).
3. Between the audio thread and the main thread, **high-frequency data goes through atomics**
   (position/volume/levels) and **discrete events go through the EventBus** (load finished, track
   ended, error).

The WebSocket thread is an exception to these three but **does not break** them: it reads the
`watch` snapshot written by the main loop each tick (read-only), and inbound commands come back to
the main thread via the `EventBus` — the same path as async tasks.

### 1.3 Data flow for playing a song

```
User Enter
  └→ App::start_playback(song, start_at_ms)          update.rs
       ├→ cancel the previous song's still-running streaming download (active_stream.cancel())
       ├→ audio.mark_loading()                       state immediately becomes "buffering"
       ├→ load_cover() / request_lyric()
       └→ request_stream()
            ├─ cache.find(key) hit ──→ audio.load(AudioSource::File(path), position)
            └─ miss → fetch direct URL (/song/url, try candidates (hash, quality) one by one)
                 └→ Loaded::StreamReady
                      └→ App::start_download()
                           ├─ start_at_ms > 0 → fetch_to() whole track → StreamCached → load(File)
                           └─ start_at_ms = 0 → start_streaming() (write .part + fill buffer)
                                └→ 128 KiB accumulated → Loaded::StreamPrerolled
                                     └→ audio.load(AudioSource::Stream(buffer))
                                └→ whole track downloaded → rename .part → Loaded::StreamCompleted (finalize only)
```

### 1.4 Cache file naming

`{hash}-{quality}.{ext}`, e.g. `b3a52a7a…-128.mp3`; different qualities don't overwrite each other.

Incomplete files are **same directory, same name + a `.part` suffix**; there are two forms, and
`cache.find` sees neither (intentional, see §3.3):

| Form | Used by |
|---|---|
| `abc-128.mp3.part` | whole-track download (`fetch_to`: resume, prefetch next track) |
| `abc-128.mp3.stream.part` | download-while-playing (`start_streaming`) |

**The two forms are deliberately different**: the same song can have both a whole-track download
(background prefetch) and a streaming download (the user just switched to it) running at once. With
a shared `.part`, the whole-track download's `File::create` would truncate the bytes the streaming
one already wrote, and data beyond the buffer window is exactly what's read back from that file via
`pread` — reading back a hole is noise. Both still end in `.part`, so `cache::is_partial` still
recognizes them.

Also in the same directory are runtime files used by scripts, named by **a convention between the
two platforms** — change one, change the other:

| File | Content | Written by / read by |
|---|---|---|
| `api-<instance>.pid` | `<PID> <port>` (space-separated) | launcher and `kugou-api` write; `kugou-api stop/status` read |
| `api-<instance>.log` | the service's stdout (Windows also has `.log.err`) | launcher and `kugou-api` write; `kugou-api logs` read |

There are only two instance names: `standard` (standard edition / NetEase) and `lite` (Kugou
Concept edition).
**Why the PID file carries the port**: with only the PID, an old process whose port was changed
without a restart makes `status` report "running" on the new port, when it actually listens on the
old one — hit in practice (see the comment on `pid_of` in `kugou-api`).

### 1.5 Opening a playlist (two-phase load)

```
Open playlist
  ├→ first_screen_page()          ← decides which page to fetch first (consistent with sort_descending)
  │     └→ fetch one page → Loaded::PlaylistTracks → set_songs_sorted(…, descending)
  │           (when displaying descending, `set_songs_sorted` reverses this page)
  ├→ needs_full_fetch()           ← whether the full table still needs filling in
  └→ concurrently fetch all remaining pages in the background → Loaded::PlaylistTracks (full-table overwrite)
```

Two rules worth remembering:

* **The first-screen page must match the display direction**. Default is descending → the last
  page's content appears at the top, so the first screen must fetch the last page; otherwise the
  user first sees songs from the middle of the list, and when the full table arrives a few seconds
  later the screen flips wholesale.
* **"Is there more" cannot be judged by whether this page is full** (see §5 "List pagination").

---

### 1.6 Where the platform branches are

There are only a handful of `cfg`s in the code, but each corresponds to a real difference, easy to
miss when changing things. The list:

| Location | Difference | Why it can't be unified |
|---|---|---|
| `[target.'cfg(windows)']` / `[target.'cfg(unix)']` in `Cargo.toml` | TLS backend (SChannel / rustls), `zbus`, `libc`, `windows-sys` | rustls on Windows drags in `aws-lc-sys`, which needs CMake + NASM installed; D-Bus doesn't exist on Windows |
| `audio/streaming.rs` | `read_at` wraps: Unix `pread` / Windows `seek_read` | Windows has no `pread` |
| `logger.rs` | stderr redirect: Unix `dup2` / Windows `SetStdHandle` | Windows has no fd table |
| `main.rs`, `app/mod.rs`, `app/update.rs` | `mpris` / `tray` modules, fields, sync logic | see above (D-Bus) |
| `config.rs`, `app/settings.rs` | paths all go through `dirs`; only `~` expansion needs an extra fallback | no `HOME` on Windows |
| `ui/icons.rs` | Nerd Font detection via `fc-list`, absent on Windows → falls back to ASCII + `KUGOU_TUI_NERD_FONT` override | there the font list is in the registry |
| `api/model.rs`, `event.rs`, `keymap.rs` | three `#[cfg_attr(not(unix), allow(dead_code))]` | those items are only constructed by MPRIS; keeping `allow` rather than cfg-ing them out keeps the enum/model shape identical on both sides, so downstream `match` needs no platform branches |
| `scripts/*` (bash) | no platform branches; instead **only constructs available on both** | the scripts run the same on Linux and macOS; per-item differences in §1.7 |
| `scripts/*.ps1` | `$env:OS -eq 'Windows_NT'` check (`-WindowStyle` etc. exist only on Windows) | `$IsWindows` is a PowerShell 6+ automatic variable, and Windows ships 5.1 |

Verifying the Windows side **doesn't need a Windows machine** (for the compile-time-checkable part):

```bash
rustup target add x86_64-pc-windows-msvc
cargo check   --target x86_64-pc-windows-msvc --all-targets
cargo clippy  --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```

`--all-targets` also checks test code. **The final link step needs Windows or the MSVC toolchain**;
on Linux, reaching `error: linker link.exe not found` means the Rust side fully passed (dependencies
and the crate itself all compiled; only linking remains).

The runtime part is covered by `.github/workflows/ci.yml`: **Linux / Windows / macOS three lanes**
each run `clippy -D warnings + test`. **Don't delete it** — path expansion, config directories, and
cache file naming only surface when actually run. The macOS lane matters for the same reason: it
shares the `cfg(unix)` branch with Linux, but `dirs` gives `~/Library/...`, audio goes through
CoreAudio, and `fc-list` is unavailable — unrun, it's only "theoretically working".

These three lanes **only verify**: no `cargo build --release`, no release packages, no artifact
upload. Three-platform release assets are produced by `release.yml` on tag push; the rationale is in
the comment at the top of `ci.yml`.

### 1.7 Launcher scripts: cross-platform conventions

The bash scripts under `scripts/` run **the same** on Linux and macOS; the PowerShell ones only make
sense on Windows (but the release zip includes the bash ones too, for Git Bash / WSL).

| Windows | Unix side | Notes |
|---|---|---|
| `kugou-tui.ps1` | `kugou-tui` | launcher. **Deliberately has no `param()` block**, otherwise PowerShell treats `-s 海阔天空` as a misspelled parameter name; all args pass through verbatim via `$args`. `--dry-run` only prints the decision |
| `kugou-api-install.ps1` | `kugou-api-install` | only clone + install deps, **does not start** — starting the service is the launcher's job, avoiding two copies of that logic |
| `kugou-api.ps1` | `kugou-api` | start/stop management (`start` / `stop` / `restart` / `status` / `logs`). Shares the PID file with the launcher, see §1.4 |
| `build-windows.ps1` | `make-release-tarball` | build + package zip. **Which scripts go in the package is decided by "mentioned in the docs"**, not by platform alone — v0.4.3 missed the `.ps1` ones, while the docs told users to run exactly the missing ones |

#### bash side: constructs not allowed

macOS ships BSD tools plus **bash 3.2**; every item below breaks the script outright (not "slightly
different behavior"). The three scripts are each distributed independently (AUR installs into
`/usr/bin`, release packages put them in `scripts/`, some people symlink them separately), so
**there is no shared library**: this restriction is restated in each script — change all three
together.

| Not allowed | What happens | Use instead |
|---|---|---|
| `readlink -f` | macOS reports `illegal option -- f` | resolve symlinks layer by layer (see `self_path` / `self_dir` in each script) |
| `dirname --` | BSD treats `--` as the argument itself | drop the `--` |
| `seq` | GNU coreutils, absent on BSD | `while [ "$i" -lt N ]` counting loop |
| `setsid` | util-linux specific | `nohup` (still the same session, but enough to outlive the terminal) |
| `ss` | iproute2, absent on macOS | `lsof -nP -iTCP:<port> -sTCP:LISTEN -t` |
| `declare -A` | bash 3.2 reports `declare: -A: invalid option` | `case` dispatch + `${!name}` indirect expansion |
| `"${arr[@]}"` (empty array) | bash 3.2 + `set -u` reports unbound variable | `"${arr[@]+"${arr[@]}"}"` |

**Directories must align with the `dirs` crate** — this is not a style issue:

| | Linux | macOS |
|---|---|---|
| config root | `$XDG_CONFIG_HOME` / `~/.config` | `~/Library/Application Support` |
| cache root | `$XDG_CACHE_HOME` / `~/.cache` | `~/Library/Caches` |

On macOS, `dirs` does **not read** XDG variables, so the scripts must not either. The consequence is
concrete: the launcher looks for a `config.toml` under `~/.config` that never exists, so it always
probes with defaults (`:3000`, standard edition), and Concept-edition users just see "service failed
to start". The config root can be overridden wholesale with `KUGOU_TUI_CONFIG_DIR` (all three scripts
+ both ps1 accept it, semantics matching `config_root()` in `config.rs`).

#### PowerShell side: four notes

* **Use only PowerShell 5.1 syntax** (what Windows ships). Don't use `$IsWindows` (6.0+ only) or
  `Start-Process -Environment` (7.4+ only). Platform detection uniformly uses
  `$env:OS -eq 'Windows_NT'`.
* **Files must carry a UTF-8 BOM**, and this **can only be guaranteed by hand** — CI won't catch it.
  Without a BOM, 5.1 decodes `.ps1` using the current ANSI code page (GBK on Chinese Windows), and
  these scripts contain dozens of user-facing Chinese messages that all turn to mojibake. PowerShell
  7 reads UTF-8 by default, and CI uses exactly `shell: pwsh` (7.x), so **green CI doesn't mean it's
  fine for users**. After adding or rewriting a `.ps1`, use
  `head -c 3 scripts/*.ps1 | od -An -tx1` to confirm every file starts with `ef bb bf`; LF line
  endings are fine (5.1 handles LF scripts).
* `Start-Process` **does not allow** redirecting stdout and stderr to the same file, so there are two
  service logs: `api-<instance>.log` and `api-<instance>.log.err` (instance names
  `standard`/`lite`/`netease`); on error, dump both.
* The config parsing in the launcher (reading `sources.active` and `[sources.<kind>].api_base`) is a
  hand-written regex, not a TOML parser. Change the config structure, change it too — likewise, the
  commit pinned in `kugou-api-install.ps1` must match the bash version's (pinning one and following
  master in the other is the worst combination).

The "Building and running on Windows" section of `docs/INSTALL.md` lists the platform capability
table (what's degraded, what's missing); after changing platform branches, sync that table too.

### 1.8 Splitting `update.rs`: progress and rules

`update.rs` was once a 4700-line `impl App` with too many responsibilities. The goal of the split
**is not to reduce line count** but to lower the cost of "how much context must be read to change
one thing". Per the audit's order, reliability issues were fixed first (Range validation in
`download.rs`, the stale-result criterion set `accepts_*`), then the structure moved — fixing
reliability first is how you learn where the real boundaries are.

Done (each block is a **pure move**, zero logic changes, compared character by character after
moving; the line counts below are current values, and each module grew again after the split):

| Module | Lines | Content |
|---|---|---|
| `navigation.rs` | 275 | page switch, focus, selection movement, `go_back`, number keys, `v` key to open the source page |
| `playback.rs` | 370 | start / track change / progress / volume / mute / lyric offset |
| `search.rs` | 142 | submit search, load more |
| `desktop.rs` | 160 | MPRIS / tray snapshot, window control |
| `cloud.rs` | 1160 | login / source switch / VIP / cloud playlists / account profile |
| `settings.rs` | 760 | settings-page candidate values and three interactions (`move_settings` / `click_setting` / `adjust_setting`) |

`update.rs`: 4728 → 4144 → 2958 lines (after the split it grew with features to the current 3679).

**Boundary corrections (found and fixed after splitting playback)**: `activate()` and
`startup_search()` were moved back into `update.rs`'s dispatch layer. `activate` was originally
split into `navigation.rs` with the "navigation" section, but it is "the Enter key deciding what to
do based on `(Tab, Focus)`" — spanning search / playback / data loading. The result was navigation
calling playback and search, making `navigation ↔ search` **leaf-to-leaf dependency**.
`startup_search` (the startup glue for `--search`: switch page + fill query + submit) is the same —
leaving it in `search.rs` would make search depend on navigation.

Dependency shape after the move (**this is the target to defend**):

```text
update ──→ navigation / search / playback / cloud / desktop / settings / mod   (dispatcher)
navigation ──→ update (ask the loading layer for data), settings (settings-page cursor movement)
playback ──→ update (client_for / load_cover / request_lyric / sync_queue_cursor)
settings ──→ update (refresh_cache_usage)
cloud ──→ update (client_for), mod (ensure_device_fingerprint)
desktop ──→ (none)
search ──→ (none)
```

There are zero calls between leaves, and **no mutually-calling pair**. What remains in `update.rs`
is three kinds of things: dispatch (`handle_event` / `handle_action` / `activate`), the loading
layer (`load_*` / `open_selected_*`, which `navigation.rs` only asks to fetch data), and results and
finalization (`handle_loaded` / `handle_audio_event` / `tick`).

**The "playback control" section is not clean, so it was not moved wholesale.** Five methods in it
don't belong to playback; the first three later went to `desktop.rs`, the other two stayed in
`update.rs`:

| Not in playback.rs | Why it isn't playback |
|---|---|
| `sync_mpris` / `sync_tray` → `desktop.rs` | output adapters, called once per tick by `tick()` to push state to D-Bus components |
| `toggle_window` → `desktop.rs` | goes through niri's compositor IPC, unrelated to audio |
| `client_for` | a client-construction helper shared by the whole file (8 call sites); moving it would only make callers write `pub(super)` everywhere |
| `request_lyric` | the "fetch lyrics" network request; its result handling is in `handle_loaded`, and keeping request and handling in one file saves context (`load_cover` likewise; both now live in the "cover and lyrics" subsection of `update.rs`) |

**This is the criterion for judging a split**: where a section's methods are called from and who
handles their results matters more than which heading they sit under. Better a smaller module with
clean boundaries than dragging neighbors along to pad size.

After a round of splitting there is one **deliberately accepted** cross-module call:
`navigation.rs`'s `move_selection` calls `settings.rs`'s `move_settings`. The settings-page cursor
movement belongs to `settings.rs` ("the settings page's three interactions" should be together), so
the navigation side merely asks it to move; it doesn't call back into navigation, and
`settings_cursor` details aren't spread across two files. Think this through before changing it;
don't move `move_settings` back to `update.rs` just to remove one edge in the graph — that would
truly dump a settings-page behavior into the "everything goes here" file.

The large classification before splitting `cloud.rs` (login / source / VIP / cloud playlists /
common loading helpers) is done, landing as four subsections with headings in `cloud.rs`; helpers
everyone uses, such as `client_for` and `request_lyric`, stay in `update.rs`.

You'll hit one thing while splitting, so know it in advance: **Rust method privacy is per "module
where the impl block is written"**, not per type. Once a method moves, calling it from the other side
reports `E0624 method is private`, and each must be changed to `pub(super)`. This isn't a design
problem, it's an inherent cost of splitting — the compiler lists them all, just follow it (navigation
13, playback 16, desktop 3, cloud 15, settings 1). Note **both directions must be changed**: moved
ones must be callable by their original callers, and ones left in place must be callable by the moved
block.

A few rules:

* **Split one block at a time, then immediately `clippy -D warnings` + `cargo test`**. Moves don't
  change logic, so all tests (currently 703) must stay green; if they aren't, the move was wrong,
  not "conveniently fixed".
* **Moves must leave evidence**. Method: copy the original file to `/tmp` before moving, then after
  moving run a script that "compares signatures by function name + function bodies (whitespace
  collapsed to a single space, `pub(super)` normalized)", confirming character by character that
  every function is still alive. This round: all 116 functions survived character for character; the
  only deletion was the duplicate quality-label function in `update.rs` (see §6).
* **Don't bundle changes into the move commit**. Mixed together, nobody can tell which line is a
  behavior change.
* `handle_loaded` (event dispatch) **stays in `update.rs`**: it's the scheduler, and the criterion
  for "should this result be accepted" stays there too (see §4 item 11); only pure business actions
  move.
* **The cut easily eats section headings**. With `sed -i 'A,Bd'`, one line too many or too few leaves
  an orphan `// =====` or takes `// 播放控制` along. After cutting, immediately `sed -n` the seam and
  `grep -n '^    // [^=]'` to check the remaining section headings.
* **After splitting, run a module call graph; you want a star.** Move-equivalence doesn't mean
  correct boundaries — a wrong boundary doesn't error, it just moves coupling from inside a file to
  between modules. Method: take the method names defined in each module, then count occurrences of
  `self.<Y's method>(` in `src/app/X.rs`. **Leaf ↔ leaf** means a wrong boundary; go back to that
  function (usually a "dispatch by Tab" one). The `navigation ↔ search` case above was found this
  way.
* **Methods in a section don't necessarily belong to the same thing**. The criterion is "where's the
  caller, who handles the result", not "which heading it's under" (see the table above).
* Constants follow their users: `RESTART_THRESHOLD_MS` moved into `playback.rs`, `SEARCH_MAX_PAGES`
  into `search.rs` — they're used only by that block.

### 1.9 WebSocket service (`ws.rs`)

Protocol aligned with MoeKoeMusic (<https://music.moekoe.cn/zh-CN/websocket-api.html>), listens on
`127.0.0.1:6520` by default, disabled by `--no-ws` / `ws = false`, port changed by `--ws-port`.

```
main loop(tick) ──sync_ws()──→ watch::Sender<Arc<Snapshot>>   （writes the snapshot only）
                                        │
ws thread ──watch::Receiver─────────────┘  diff_messages() → broadcast → each connection
    │
    └── inbound control ──EventBus──→ main loop dispatches into an existing Action
```

A few design trade-offs, read before changing:

* **Own thread + `current_thread` runtime**, not stuffed into the main runtime. It does two things:
  send/receive WebSocket frames, and compare snapshots. The main loop's tick cadence shouldn't be
  affected by network I/O.
* **`sync_ws` writes a full snapshot each tick** (including a `Song` clone); `send_if_modified` uses
  `Snapshot`'s custom `PartialEq` — it compares only the fields that "would change the pushed
  content", deliberately not the whole `Song` (`privilege` is a large nested JSON, not worth
  comparing every tick). If the snapshot is unchanged, the broadcast task isn't woken, making this
  call nearly free while paused.
* **`playerState` is pushed only on play/pause transitions**, not every tick. Upstream's only call
  site of `updatePlayerState` is `play-pause-action` in `electron/main.js`; the docs say "when the
  playback state changes". Sent once more separately when a connection is established.
* **`lyrics` is pushed every tick** (upstream's `server-lyrics` path is likewise undebounced; only
  the desktop-lyrics IPC debounces per line). `lyricsData` is the **raw lyric text**, i.e.
  `Lyric::text` — not the parsed `lines`, because the word-by-word markers and `[language:]` tags
  are exactly what third-party clients need to lay out themselves.
* **welcome's `data` is a plain string**, not the nested object in the doc's example. The source is
  authoritative.
* **Unknown `control` commands get an `error` reply**, unlike upstream which silently ignores them.
* **Handshake validates `Origin`**: any page in a browser can connect to a local port, and without
  validation that opens playback control to every tab open at the time. Native clients without an
  `Origin` are allowed.
* **Inbound messages capped at 64 KiB** (tungstenite's default 64 MiB is too loose). Outbound is
  uncapped; KRC can be tens of KB.
* **The `lyrics` variant is `Box<LyricsData>`**: unboxed, each enum instance is 264 bytes, and it's
  passed by value in a `Vec`.

### 1.10 How to verify the WebSocket

No automated integration test (it needs a real player running); verify by hand with an external
client:

```bash
# 1) start the player (isolate the config dir! don't touch ~/.config/kugou-tui)
KUGOU_TUI_CONFIG_DIR=/tmp/kt-ws ./target/debug/kugou-tui --search 周杰伦 --no-tray
# 2) in another terminal, connect any WS client to ws://127.0.0.1:6520/
#    expected: welcome + playerState first, then one lyrics message every 200ms during playback;
#    sending {"type":"control","data":{"command":"toggle"}} should toggle playback and yield a playerState.
```

`node --experimental-websocket` or a browser console works; no extra dependencies needed.

### 1.11 Embedded API layer (`api/native/`) and KAT

`--api native` (default) implements KuGou's 26 routes in-process, **no longer needing Node.js**. This
layer's risk is "wrong results without errors": if a single byte of the signature, device
fingerprint, or KRC decryption is off, the server returns a response that looks like a business error
(or an empty body), and it's hard to locate which step was wrong.

The countermeasure is **KAT (known-answer test)**: compute baseline values on the upstream JS with
fixed inputs, and assert byte-for-byte equality on the Rust side. Baselines **are not hand-copied
constants** but are produced fresh by the scripts under `tools/kat/` (dependencies, usage, known
pitfalls in [tools/kat/README.md](../tools/kat/README.md)); when upstream changes an algorithm, or you
need to recheck a signature, rerun the same script to compare.

Rules when changing anything under `api/native/`:

- **Each function notes its source file and function name in a comment** (ported function by
  function from upstream), for later cross-checking.
- **Changing an implementation means rerunning the corresponding KAT script and updating expected
  values if needed** — changing only the Rust without changing the baseline makes the test either red
  or falsely green.
- The scripts in `tools/kat/` **don't participate in the build and aren't test fixtures**, purely an
  archive of "where the baselines come from".

The Soda source's data plane is another crate (`crates/libresoda`), behavior-aligned with the
official SodaM client, so for problems there **first check whether upstream fixed it**; don't write a
second copy in this repo.

---

## 2. Running / verifying

```bash
cargo test                         # unit tests (~700, seconds)
cargo clippy --all-targets         # the release script runs this
cargo build --release              # binary ~11.7 MiB
./target/release/kugou-tui --print-config   # show effective config, log path, cache dir
```

**Smoke test** (with a real sound card and API service on the machine):

```bash
# 1) start the API service (first time needs ./scripts/kugou-api-install)
./scripts/kugou-api              # or the launchd/systemd setup

# 2) play a song with a clean cache and watch the log
KUGOU_TUI_DEBUG=1 ./target/release/kugou-tui --search "周杰伦 晴天" --no-tray \
    --cache-dir /tmp/smoke-cache
# then: Esc → Enter to play → n to change track → Space to pause/resume → q to quit
# log: ~/.cache/kugou-tui/kugou-tui.log
```

To **isolate** your experiments from the real config and session, use XDG variables to relocate:

```bash
XDG_CONFIG_HOME=/tmp/xdg/config XDG_CACHE_HOME=/tmp/xdg/cache \
    ./target/release/kugou-tui --print-config
```

> Why: `session.json` (last position, queue) lives in `XDG_CACHE_HOME/kugou-tui/` and is
> **overwritten on exit**. Running automated tests against the real config clobbers the user's queue
> cursor and playback position. Either isolate, or back up `~/.cache/kugou-tui/session.json` first.

---

## 3. Troubleshooting guide for two hard classes of problems

### 3.1 Memory only grows

**Symptom**: after continuous listening and track changes, RSS passes 100 MB and doesn't come back
down after stopping.

**Measure first, don't guess.**

```bash
pid=$(pgrep -f 'target/release/kugou-tui' | head -1)
grep -E 'VmRSS|VmHWM|Threads' /proc/$pid/status    # current / peak / thread count
```

Sample in a loop (watch the trend, not a single point):

```bash
while :; do grep VmRSS /proc/$pid/status; sleep 1; done
```

**Two historical root causes for this class** (both can still recur elsewhere; before changing
things, confirm they haven't been reintroduced here):

1. **Unbounded buffering**. `StreamingBuffer` used to keep every byte downloaded for the whole track
   in memory (a `Vec`-doubled whole track at that), so a 64 MiB Hi-Res song was 66.5 MiB. Now only a
   4 MiB tail window is kept, and the rest is `pread` back from the on-disk `.part`.
   → **Point**: for any memory structure that "grows with media size", ask "what's its upper bound".
2. **Background tasks nobody cancels**. Not cancelling the previous song's streaming download on a
   track change meant each `n` press added another running download pouring a whole track into
   memory — measured 323.7 MiB after 5 consecutive changes. Now `App::active_stream` holds the handle
   to the current stream, and `start_playback` / `stop_playback` / `shutdown` all `cancel()`.
   → **Point**: when creating a background task, ask "who's responsible for stopping it".

**Locating techniques** (ordered by cost-effectiveness):

| Technique | What it shows |
|---|---|
| Sample `VmRSS`, align with "time window + user action" | whether growth happens during playback, on track change, or while idle |
| Difference between `VmHWM` and current RSS | whether there's a one-off large peak (e.g. the moment a whole track finishes) |
| `Threads:` count | whether threads/tasks leak (normal 7–9) |
| `KUGOU_TUI_DEBUG=1` and watch `预取` / `缓存回收` / `流式下载` logs | which chain is running and whether it finished |
| Control experiment: temporarily set `MAX_KEPT_BYTES` to 64 KiB | if RSS drops accordingly, that buffer is the bulk |

**Don't do**: periodic `malloc_trim`, periodic `shrink_to_fit`, GC-style "reclaim threads". Those
only hide the growth; the peak and the bound remain. Change the **bounds and ownership** instead.

### 3.2 Playback jumps back to the start / skips songs while playing

**Symptom**: during download-while-playing, after a buffer it replays from the beginning; or it
skips to the next song mid-listen; or it loops every ten-odd seconds.

**First distinguish "finished" from "broken"**. The two look identical at the playback layer,
because:

> `rodio`'s decoder swallows **any** read error as EOF
> (`symphonia`'s `format.next_packet().ok()?`), so a source "ending" may be a real finish
> or a broken stream.

So `engine::Runtime::sync()` **cannot treat an empty player as finished**; it must grade by the
stream's state (`classify_drain`):

| Real stream state | Wrap-up | What the user sees |
|---|---|---|
| `is_cancelled()` | silent | track changed, nothing should pop up |
| fully downloaded | `TrackFinished` | normal advance to next track |
| not finished + has `error` | `Failed(reason)` | report error, stop, don't change track |
| not finished + no error (read timeout) | `StreamInterrupted { position }` | stay put, auto-resume once from that position |

**Two historical root causes for this family**:

1. **Reloading after finishing** (`Loaded::StreamCached` treated as "time to play"). The song is
   already playing; reloading via `audio.load()` resets position to 0. Now the streaming path goes
   through `Loaded::StreamCompleted`, which only finalizes and doesn't touch the player.
2. **A broken stream treated as finished**. Under single-track repeat (or a queue with one song), a
   track change means "the same song replays from 0". Now handled by the grading above; the
   **read-timeout** case also preserves the position and auto-recovers once.

**One more detail you must know**: the position must be recorded **before the track ends**.

```rust
// the final frame cannot read get_pos(): the source is gone, rodio reports 0
let position_ms = self.last_position_ms;   // the value from the last frame still playing
```

Otherwise the progress bar jumps back to 00:00 at the moment of finishing, and the later "resume"
has no position to resume from.

**Reproduction method** (real machine, repeatable): use a "cut-the-stream" proxy to drop the CDN's
data mid-transfer for 20–30 seconds, leaving the rest to real decoding and playback. Key points:

1. pick a small quality (`quality = "128"`, a 4 MB song) so the download can "finish" or "break"
   mid-playback;
2. pick playback mode `repeat_one` — it magnifies "replay from the beginning" into something visible
   (sequential playback would just look like "skipping this song");
3. observe **with the app's own state** (`position_ms` in the `session.json` written on exit, or the
   log line 「会话已保存：N 首，位置 X ms」), not MPRIS `Position`:
   **when the player state is Stopped, `playerctl position` always reports 0**; that's not the truth
   and will lead you astray.

**Another trap**: half a song. If the streaming download wrote directly to the real cache filename, a
failure/cancel leaves a "seemingly complete" short file on disk, and `cache.find` only checks "does
the file exist", so the next play hits it → EOF halfway → judged finished → single-track repeat
restarts → looping every ten-odd seconds. **So `.part` + rename-only-when-done cannot be skipped.**

---

## 4. Invariants when changing this code
Before changing anything under `audio/`, confirm these aren't broken:

1. **Write to disk first, then `push`**. The buffer keeps only a window, and bytes outside it can
   only be read back from `.part`; reverse the order and you read a hole that hasn't been written
   yet (heard as noise).
2. **The on-disk file must be opened read-write** (`OpenOptions::read(true).write(true)`).
   `File::create` gives only `O_WRONLY`, and `pread` on such a handle is `EBADF`. Hit once.
3. **Distinguish read position from write state**: `is_finished()` (task done) ≠ `is_complete()`
   (whole track intact). The playback engine relies on this to tell "finished" from "download
   failed".
4. **Raise errors only when a read is truly impossible**. The already-downloaded part still plays
   fine — a later failure must not cut off what the user already heard.
5. **Cancellation must wake blocked readers**. `read()` blocks on a condvar; `cancel()` must set the
   flag and `notify_all`, otherwise a track change waits until the next read timeout (up to 15
   seconds) to release, while the audio thread is waiting in `Player::clear()`.
6. **`.part` is not cache**. Reclamation and "clear cache" must skip it (`cache::is_partial`).
7. **Don't read `get_pos()` after the end** (see §3.2).
8. **End/failure/cancel must all drop the handle**: `active_stream` (App), `Runtime::stream`
   (engine). Keeping one drags the whole buffer and file handle along.
9. **Don't change `WAIT_TIMEOUT` to fix memory**. 15 seconds is the fallback for "the network is
   truly broken and we must report an error"; raising it only makes hangs longer, lowering it
   misjudges normal waits.
10. **`Output`'s field order can't change** (the player must precede the device), and don't delete
    the `_stream` field — it works via lifetimes, and deleting it kills the sound immediately.

Before changing `handle_loaded` in `app/update.rs`, this must hold too:

11. **Every async result must prove its identity**. Requests are all spawned with `tokio::spawn`, and
    return order isn't guaranteed; the user can search again, open another playlist, or click
    another artist in between. Accepting results unconditionally produces states like "the input box
    says B, the list is A" that **look like data errors**, without any error being reported.

    The criterion is "identity comparison", not numbering each request: the playback side already
    does this (`App::is_current` compares `song.hash`, lyrics compare `hash`); the rest just copies
    the same idea. Existing comparison points:

    | Result | Compared against |
    |---|---|
    | `StreamReady` / `StreamPrerolled` / `StreamCached` | `song.hash` (`App::is_current`) |
    | `Lyric` / `LyricFailed` / `CoverReady` | `hash` |
    | `Search` | keyword (`state.search.submitted`) |
    | `PlaylistTracks` | the currently open playlist id (`open_playlist`) |
    | `ArtistSongs` / `RankTracks` | the currently open artist / ranking id |
    | `Playlists` / `Artists` | the category id at request time (`category` / `kind`) |

    The criteria themselves are extracted as free functions (`accepts_search_result` /
    `accepts_open_item`), because they can be tested standalone, whereas constructing `App` needs an
    audio engine and runtime, which unit tests can't build.
    **When adding a `Loaded` variant that carries a network request, first decide what it compares
    against on return.**
    Also, identity fields must be written into state **before `spawn`** (`open_playlist` /
    `open_artist` / `open_board` all do this) — writing them in the result is too late, and that
    result will never match.

Before changing `audio/downmix.rs` (multichannel downmix), this must hold too:

12. **Multichannel files must be downmixed before reaching rodio.** rodio's
    `ChannelCountConverter`, when reducing channels, "keeps the first N samples of each frame and
    discards the rest" (`conversions/channels.rs`; in the unit test `remove_channels`, 4→1 yields
    `[1.0, 5.0]`). And KuGou **has multichannel lossless files**, possibly of the kind where "the
    first two channels aren't this song": measured, the `flac` version of 《东京不太热 (DJ Z新豪版)》
    is 4.0, with the song in the last two channels (the first two are 6–10 dB lower and correlate
    only +0.35 with the correct mix). Without this layer, the user hears something other than this
    song, showing up as "the audio version differs from other clients". The criterion and measured
    data are at the top of the `downmix` module; both exits of `build_decoder` must pass through
    `downmix::to_stereo`. Two guard tests fail when this layer is removed:
    `decoded_multichannel_files_are_downmixed_to_stereo` (a hand-made 4-channel WAV through
    `build_decoder`, asserting `channels() <= 2`) and
    `the_mixer_hears_the_song_only_because_we_downmixed_first` (builds a sound-card-independent mixer
    with `rodio::mixer::mixer`, proving by contrast that "going straight into the mixer keeps only
    the first two channels; downmixing first is correct" — pinning exactly the layer where the bug
    occurs).

Before changing `app/desktop.rs` (syncing playback state to desktop integrations), this must hold
too:

13. **Compare first, clone later, every tick.** `tick` calls `sync_ws` / `sync_mpris` / `sync_tray`
    every 200ms, and they only take `&self.state`: do **not** `clone()` the current track, artist
    list, or whole lyrics before handing them to a handle for comparison. `WsHandle::update_from` /
    `MprisHandle::update_track` / `TrayHandle::update_track` all "compare `hash` / `track_id` and
    progress first, rebuild metadata only if changed"; passing a reference + closure is enough;
    tracks compare the **raw hash string** (not a digest), byte-for-byte equal counts as unchanged.
    Guard tests: `update_from_wakes_only_when_the_snapshot_changes`,
    `update_track_skips_metadata_rebuild_for_the_same_track`.

Before changing the stream-fetch failure path in `app/playback.rs`, this must hold too:

14. **A load failure must clear the "buffering" state.** `start_playback` first calls `mark_loading()`
    to set `PlaybackState` to `Loading`, but the engine won't change it when "nothing is loaded"
    (`sync()` sees `!loaded` and returns immediately). If a stream fetch fails (`request_stream`
    can't reach the target source, or the async `song_stream_url` fails via `Loaded::Failed`) and
    nothing handles it, the state stays `Loading` forever — and `toggle_playback` precisely ignores
    `Loading`, so the user presses Space with no response, looking like a hang. Both paths must clear
    it: synchronous errors via `App::abort_loading`, async failures via `audio.stop()` in
    `Loaded::Failed` when `playback == Loading`; only act while `Loading`, to avoid wrongly stopping
    a song that's playing (failures other than cover/lyrics go this way too).

Before changing the stream-fetch-failure track skip in `app/playback.rs`, or cloud-playlist writes in
`app/cloud.rs`, these two must hold too:

15. **Only source-level stream-fetch failures auto-skip; single-song-level failures don't.** Stream
    URL resolution failures fall into two classes: source-level (can't connect, not logged in, local
    service not started — `is_connectivity` / `is_auth_related` / `Service` / `NonJsonBody`) and
    single-song-level (copyright-restricted, VIP trial without permission, etc.). Only source-level
    goes `Loaded::StreamFailed` → `App::handle_stream_failed` → auto-skip to the next playable song
    in the queue; single-song-level still only reports the error and `abort_loading`. The rules are
    all in the pure function `auto_skip_next`: single-track repeat (`RepeatOne`) doesn't skip; a
    source already confirmed failed in the same round is skipped directly, without re-sending
    requests doomed to fail for its subsequent songs; a consecutive skip run is at most
    `min(queue length, MAX_AUTO_SKIP = 5)`; only search forward, never wrap around; if all fail, stop
    in `Stopped` with one summary message. **Any manual user action (track change, pause, play a new
    song) calls `reset_auto_skip()` to zero the counter** — `play_from` / `play_from_queue` /
    `toggle_playback` / `next_track` / `previous_track` / `stop_playback` all call it. Playback
    stream-fetch failure uses `Loaded::StreamFailed` separately (not `Loaded::Failed`), because a
    prefetch stream-fetch failure has exactly the same message as playback, indistinguishable by
    text. Guard tests: `auto_skip_*` (pure functions) and `stream_failure_*` /
    `single_song_failure_does_not_skip` / `manual_track_change_clears_the_auto_skip_state`.

16. **Cloud-playlist add/remove/sync only accepts songs from the same source as the playlist.** The
    playlist API picks a client by "the playlist's source" (`active_source_kind()`); feeding it a
    song from another source makes KuGou locate by fileid and NetEase by hash a nonexistent (or
    someone else's) entry, corrupting the playlist. So `remove_focused_song_from_cloud` /
    `add_focused_song_to_cloud` / `sync_queue_to_cloud` all go through `playlist_accepts_song`
    (`song.source == playlist's source`): cross-source is refused outright with a message and no
    request is sent; batch operations sort per song with `partition_by_playlist_source`, sending only
    same-source ones and stating the skipped count in the message. **No cross-source conversion**
    (finding a same-named song on NetEase for a KuGou song) — that's a separate feature. Guard tests:
    `playlist_guard_*` / `partition_*` / `adding_a_cross_source_song_is_refused_before_any_request` /
    `syncing_a_mixed_queue_only_sends_matching_songs`.

---

## 5. Common pitfalls

**Playback / rodio**

- A decoder read error = EOF; no exception can propagate up. Any "stream broke" judgment must
  inspect the buffer state itself.
- After `Player::stop()`, `get_pos()` resets to zero (explicitly written as 0 in `periodic_access`);
  `Player::clear()` also calls `pause()`.
- `Player::clear()` waits for the current source to exit (`sleep_until_end`). If the source blocks
  forever in `read()`, the audio thread hangs with it — so the buffer's read timeout must exist.
- The `LevelMeter` wrapper must forward `try_seek`, otherwise the whole audio becomes unseekable
  while playback itself is perfectly normal (extremely hard to notice).
- With one song in the queue + single-track repeat / list repeat, "finished" and "replay from the
  start" look the same; easy to mistake a bug for a feature when debugging.
- **Multichannel files get "channels dropped" in rodio.** Channel reduction keeps only the first N
  samples per frame and discards the rest (`conversions/channels.rs`). KuGou has multichannel
  lossless files, possibly of the "song in the last two channels" kind (see §4 item 12).
  `build_decoder` now uniformly goes through `downmix::to_stereo`; don't remove that layer.
  Diagnosis: `ffprobe` the `channels`, then compute per-channel correlation with the correct mix.
- **`/song/url` silently downgrades while `status` stays 1.** Measured, requesting `quality=flac`
  for a song without a lossless version returns `extName=mp3` + `bitRate=128000`. Judging only
  `status` leaves the UI showing flac while actually playing 128 kbps. The criterion is in
  `catalog::downgrade_note` (coarse ordering by container + bitrate; `super` and the viper series
  don't participate), and the reason is surfaced to the UI via `StreamUrl::reason` — note that in
  `handle_loaded` **both branches of `reason` must be handled**; handling only the trial branch
  swallows the downgrade notice.

**Download / cache**

- `cache.find` only judges by "does the file exist" and doesn't validate length — that's the reason
  `.part` exists.
- **Every step of a chunked download must prove itself**: the status code must be `206`,
  `Content-Range` must match the requested range byte for byte, each chunk must write its full
  length, and the total must equal the total length. Miss any one and when the server (or an
  intermediate CDN / proxy) ignores `Range` and returns `200 OK` with the whole track, the 4 chunks
  overwrite each other and assemble a "correct length, entirely wrong content" file — which still
  gets renamed into the cache. When writing your own proxy for experiments, don't forget to pass
  through `Range`. Regression tests: `server_ignoring_range_*` and `truncated_transfer_*` in
  `audio/download.rs` (a fake server on a std `TcpListener`, no new dependencies).
- `.part` is pre-allocated with `set_len(total)`, so **file size doesn't prove integrity**, only the
  number of bytes written does. Hence "download task finished" ≠ "download succeeded".
- A failed chunked download **falls back to a single connection** and re-downloads (`fetch_to`), so
  "it looks like it downloaded twice" may be normal.
- Cache reclamation is a synchronous directory scan and must run in `spawn_blocking`; don't block the
  UI.
- **Two streams must not run against the same target at once.** `start_streaming` deduplicates in a
  registry keyed by temporary file path: the second reuses the first's buffer and starts no task. The
  reason is that two streams corrupt each other — the later `truncate(true)` wipes the bytes the
  earlier wrote, and `remove_file` on failure/cancel deletes the other's file. It doesn't take an
  unusual action to trigger: **pressing Enter twice** puts two `start_streaming` calls on the same
  target (`App::active_stream` isn't held until enough of the head is buffered, and nothing blocks
  the second during that window). Regression test:
  `audio::download::tests::a_second_stream_for_the_same_target_reuses_the_first_buffer`.
- Whole-track and streaming downloads **don't share** a temp file (`x.mp3.part` vs
  `x.mp3.stream.part`, see §1.4): when prefetching the next track in the background, the user may
  well switch to exactly that one.

**List pagination**

- `pagesize` on playlist-class APIs is a **hard cap of 30**; the response's `count` is only this
  page's item count, **not the total**; the only way to compute total pages is `song_count` from the
  playlist metadata.
- "This page isn't full ⇒ that's everything" holds **only on page 1**. After the first screen
  fetches from the last page (when displaying descending), a non-full last page is normal — copying
  this over would make "fill in the full table" never fire, showing only the last page's few dozen
  songs. The criterion is extracted in `app::update::needs_full_fetch`, pinned by a test.
- More fundamental: **"is there a next page" can only be judged by whether this page is empty**.
  Parsing filters entries (missing hash, wrong field type), so a page of 30 shrinking to 29 is
  common, and stopping on "not full" silently truncates the list. **Both sides now use the same
  criterion**: KuGou in `catalog.rs::collect_all_pages`, NetEase in
  `source/netease.rs::should_continue_paging` (changed 2026-09-28, see §7). NetEase's criterion is
  extracted as a free function because its unit test can't exercise the whole paging flow (needs
  HTTP), yet the criterion itself must be pinnable — see that test's comment.
- **Concurrent page results must be assembled by page number, not completion order.**
  `JoinSet::join_next()` gives task completion order, so a batch of pages sent together appends in
  whatever order they return, making the whole table's page order random — fatal for rankings
  especially, whose "order" is the content itself. Regression test:
  `api::catalog::tests::concurrent_pages_are_assembled_in_page_order` (a hand-written fake server
  that makes page numbers **necessarily** return in reverse, so this test doesn't pass by luck).
- Which page to fetch is decided by `app::update::first_screen_page`: **must be consistent with
  `sort_descending`**, otherwise the first screen doesn't match the head of the final list and the
  screen flips wholesale (directly visible to the user).
- If the first screen gets an empty page (`song_count` stale), fall back to page 1; don't leave the
  user staring at an empty list.

**Process / environment**

- DEBUG isn't logged by default; set `KUGOU_TUI_DEBUG=1` before troubleshooting.
- `session.json` is overwritten wholesale on exit. Automated tests must either isolate
  `XDG_CACHE_HOME` or back it up first.
- MPRIS `Position` is meaningless when `Stopped` (`playerctl` reports 0).
- Running two kugou-tui instances on one machine: the later one can't take
  `org.mpris.MediaPlayer2.kugou-tui`; don't take `playerctl` readings as sole evidence.
- ALSA reports plugins (`lavrate`/`jack`/`oss`…) as devices; device enumeration already does two
  rounds of filtering (can provide a default output config + exclude `null`); don't revert to "list
  everything".
- Concept edition (lite) and standard edition handle `ppage_id` differently; **don't invent
  `ppage_id` yourself when fetching the direct URL** (see the comment in `api/catalog.rs`: a wrong
  one gives 31863 directly, showing up as "all songs unplayable").

**Lyric parsing**

- **The "does this line count as a lyric" criterion can exist in only one place**
  (`api/lyric.rs::has_lyric_content`). `parse_lrc` uses it to decide which lines enter `Lyric`, and
  `attach_translations` uses it to compute these lines' index among the **original timed lines**, to
  fetch translations from the language track (`[language:base64]`). Judging separately on both sides
  with slightly different criteria shifts the index by one, **throwing off every subsequent line's
  translation** — showing up as "translations don't match the text", with no error and no crash, only
  detectable by staring at the lyrics.
- **Text before `<` is also body text.** The standard KRC form starts each line with
  `<offset,duration,0>`, so text before the marker is usually empty; but when a song contains text
  with a `<` and no matching `>` (e.g. `宝贝<3`), taking only the text after the marker drops the
  whole line. `parse_krc_words` now collects the prefix first, and since the prefix has no per-word
  timing, `words` is cleared by that "word count must equal marker count" check and falls back to
  whole-line highlighting — a designed fallback, not a bug.
- Word-by-word info is better **absent** than **guessed**: when word count and marker count mismatch,
  clear `words` and fall back to whole-line highlighting in the UI. A guessed timing makes the whole
  line sing out of sync with the audio, worse than no effect.

**Rendering / layout**

- **`Ord::clamp` panics when `min > max`** (`assert!(min <= max)`). `x.clamp(1, area.height)` is a
  time bomb when `area.height == 0`, and it looks exactly like defensive code. Guard degenerate areas
  first with `if area.width == 0 || area.height == 0 { return ...; }`; do **not** paper over it with
  `max(1)` — the resulting box extends outside the area and `area.width - columns` then underflows.
  Hit once: `ui/views/player.rs::fit_box`.
- The converse also matters: subtractions like `area.width - columns` underflow when "the box is
  bigger than the area", so after computing a box, assert **both** `box ⊆ area`, not just "doesn't
  panic".
- **The UI layer currently has no `unwrap()` / `expect()` in production code** (all in
  `#[cfg(test)]`). A panic means the UI is gone and the terminal is left in raw mode — the most
  expensive class of failure; keep it this way.
- **Wide and short** terminals (e.g. 100×8) are the shape most likely to break layout: the wide
  branch is taken but the height is insufficient, and `Min` and `Length` compete for space. After
  changing layout, run a few such sizes (`TestBackend` + `terminal.draw`, see
  `render_home_in_a_short_wide_area_does_not_panic`). Measured, `[Min(6), Length(8)]` gives `Min` the
  win when space is short, so the cover pane still has at least 4 rows — but that's **measured
  behavior, not a documented promise**; re-test after upgrading ratatui.
- **Don't re-encode the login QR code every frame**: `render_login` reuses `login.qr` directly
  (rendered according to `config.qr_aspect` when the QR code is received, see `Loaded::LoginQr`), and
  `qr_lines_fitted` only judges "does it fit" on the already-rendered lines, no longer encoding
  itself; only when the terminal can't fit it does it encode once with `qr_needed_size`. One encoding
  measures about 0.9ms, a large chunk of the budget at 30fps (benchmarks: `bench_qr_encoding_cost`,
  `bench_login_render_cost`).

**Testing**

- Unit tests run in the same process, so **don't assert with `VmRSS`** (parallel tests and allocator
  caching interfere). To assert "memory is bounded", assert the **window length** (see
  `streaming::tests::keeps_only_a_bounded_window_in_memory`), and do RSS-level comparisons in an
  out-of-process experiment (§6).
- Disk-related tests must open temp files **read-write** (same as §4.2), otherwise the test fails with
  `EBADF`.

---

## 6. Memory control experiment (run after changing the buffer/download)

Idea: compile the **same** `streaming.rs`, old and new versions, into a small program, run the same
workload, and read `VmRSS` / `VmHWM` from `/proc/self/status`. A temporary project outside the repo,
no repo pollution:

```bash
mkdir -p /tmp/mem-repro/src && cd /home/xibie/kugou-tui
git show HEAD:src/audio/streaming.rs > /tmp/mem-repro/src/old.rs
cp src/audio/streaming.rs            > /tmp/mem-repro/src/new.rs
# in main.rs: #[path = "old.rs"] mod old; #[path = "new.rs"] mod new;
# run three workloads each: single track / N consecutive changes / consecutive changes without cancel
```

`streaming.rs` depends only on `std`, so it can be pulled in with `#[path]` directly — an extra
benefit of writing it with "no in-crate dependencies".

**2026-09 baseline data** (64 MiB single track, 64 KiB chunks, release, CachyOS):

| Workload | Before fix | After fix |
|---|---|---|
| Single track played through (peak RSS) | 66.5 MiB | 12.5 MiB |
| 5 consecutive changes, none cancelled | 323.7 MiB | 3.0 MiB (after cancel) / 53 MiB (no cancel) |
| 24 tracks played consecutively | 66.6 MiB (constant, doesn't grow with count) | 12.5 MiB (constant) |

**Real process** comparison (`--search` + keypresses in a pty + sampling `/proc/<pid>/status`): same
6 track changes, 85 MiB and monotonically rising before the fix, 25 MiB and stable after; at 12 track
changes, peak **175.8 MiB** before the fix, still 145.7 MiB on exit (no drop), peak **35.3 MiB**
after, 30.1 MiB on exit.

To judge "is there a persistent leak": **play enough songs (≥20) and see whether RSS converges to the
same value**. After the fix, 24 consecutive tracks hold RSS steady at 12.5 MiB, showing every
stream's lifetime is fully cleaned up.

---

## 7. Known but **untouched** areas

This section records things "reviewed and confirmed problematic or suspicious, but deliberately not
changed this time". Keeping them isn't forgetting; the cost or risk of changing exceeds the benefit —
read the reasons here before acting.

| Location | Problem | Why untouched |
|---|---|---|
| ~~`source/netease.rs::playlist_tracks_all`~~ **(fixed 2026-09-28, kept for the conclusion)** | ~~Stops paging on "less than a full page ⇒ end", conflicting with the KuGou side's "stop only on an empty page" criterion; any page with one entry filtered out by parsing silently truncates the whole table~~ | The blocker then was "can't measure it", now removed: started NeteaseCloudMusicApi (`:3002`) and measured out-of-range offsets — `offset=500` (the playlist has 200 songs) and `offset=999999` both return `{"songs":[],"code":200}`, **an empty array, not an error**, and it doesn't clamp offset to the last page and return duplicates. The criterion is extracted as `should_continue_paging` (`got > 0`), consistent with the KuGou side; the regression test `paging_stops_only_on_an_empty_page` does fail when reverting to the old criterion. Cost: one extra request at the end of a full-table load |
| `source/netease.rs::plaza_playlists` / `artist_list` | Category (`_category_id`) and region (`_kind`) parameters are ignored, yet the UI still shows the selectors | The UI passes **KuGou**'s category ids; NetEase wants `cat` strings / `area`, and the two don't match. Adding a mapping is a new feature, not a bug fix; first acknowledge it has no effect, so users don't think the filter is broken |
| `api/catalog.rs::request_song_url_with_hash` | `Song.album_id` / `album_audio_id` are parsed but not sent to `/song/url` | On 2026-09-28, measured four parameter combinations on the Concept-edition service (none / with `ppage_id` / with the id from search / with the id from `/privilege/lite`) and **all obtained a direct URL**, while `status_reason()` records a counterexample that "the id from privilege gives status=3". Sending them has no provable benefit and a clear cost when wrong, so none is sent; the conclusion is in the comment there |
| `api/model.rs::normalize_duration` | Guesses the unit using 10000: a 9-second short track (9000 ms) is treated as 9000 seconds; a 2.8-hour long compilation (10800 s) is treated as 10.8 seconds | Both directions collide in the 1000–9999 range, while the actual APIs' units are **known** (search `Duration` in seconds, playlist `timelen` in milliseconds); this fallback only applies to unseen shapes. Changing the threshold moves the error from one range to another, not a fix; a test pins the current behavior, revisit when it actually bites |
| `config.rs::save` | Directly `fs::write`s over `config.toml`, not "write temp file + rename"; a crash could in theory leave half a config file | The config file is only a few hundred bytes, and one `write` syscall doesn't fill a block, so in practice you get the old or the new content, not a mix; also `load()` falls back to defaults with a warning on a bad file, so the worst case is "settings lost, log in again". Doing it properly means handling Windows' difference where `rename` can't overwrite an existing file, and the benefit doesn't justify the complexity |
| `audio/download.rs` | Two **whole-track** downloads landing on the same target (prefetch vs resume) still contend for the same `.part` | Both write the same bytes from the same URL; worst case the later one's rename fails and reports one download error. The trigger chain is long (prefetch in flight → track change → that song breaks again); separating the more easily collided "streaming × whole-track" combination first is enough |

**Before changing any of these**, first see whether there's a way to verify on a real machine — the
common trait of these entries is "whether the change is correct can't be seen from unit tests alone".
