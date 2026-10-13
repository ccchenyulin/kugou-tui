# Configuration

## Command-line arguments

CLI > environment variables > config file > built-in defaults.

| Flag | Environment variable | Description |
|---|---|---|
| `--api <node\|native>` | `KUGOU_API_BACKEND` | Which backend serves the Kugou API. Default `native` (embedded pure Rust, no Node.js required). See below |
| `-a`, `--api-base <URL>` | `KUGOU_API_BASE` | KuGouMusicApi address, default `http://127.0.0.1:3000` (used only with `--api node`) |
| `-c`, `--cookie <COOKIE>` | `KUGOU_COOKIE` | Login credentials, e.g. `token=xxx; userid=xxx` |
| `-s`, `--search <KEYWORDS>` | — | Search for these keywords immediately on startup |
| `--volume <0-100>` | — | Initial volume |
| `--cache-dir <DIR>` | — | Audio cache directory |
| `--cache-limit <MiB>` | — | Cache limit; `0` means unlimited |
| `--tick-ms <MS>` | — | Refresh interval, 50–5000; larger values lower CPU further |
| `--page-size <N>` | — | Items per page for search results and the playlist square, 5–200 |
| `--proxy <URL>` | `KUGOU_PROXY` | HTTP proxy used to reach the API service |
| `--sodam-cookie <COOKIE>` | `SODAM_COOKIE` | Sodam Music login state, e.g. `sessionid_ss=…; sessionid=…` |
| `--sodam-device-id <ID>` | `SODAM_DEVICE_ID` | Sodam device fingerprint (**may be empty** when using the signer service) |
| `--sodam-iid <ID>` | `SODAM_IID` | Sodam install id |
| `--sodam-signer-url <URL>` | `QISHUI_SIGNER_URL` | App signer service URL; empty uses the built-in **public default**, `none` disables signing |
| `--sodam-signer-token <TOKEN>` | `QISHUI_SIGNER_TOKEN` | Bearer token for the signer service (empty for the public address) |
| `--sodam-x-helios <VALUE>` | `SODAM_X_HELIOS` | Static signature headers from manual capture (unused when using the signer service) |
| `--sodam-x-medusa <VALUE>` | `SODAM_X_MEDUSA` | Same as above; used as a pair |
| `--basic-color` | — | Fixed 16-color palette for older terminals |
| `--no-tray` | — | Do not register a system tray icon (or set `tray = false` in the config to disable it permanently) |
| `--no-api-start` | `KUGOU_API_AUTO_START=0` | Do not auto-start the local API service (you manage it yourself; errors out if nothing is listening on the port) |
| `--api-start` | — | Prepare and start the local API service, then exit without entering the UI; services started this way **stay in the background** and are stopped with `--api-stop` |
| `--api-stop` | — | Stop the local API service(s) started by this program |
| `--print-config` | — | Print the effective config and the cache/log paths, then exit |

```bash
kugou-tui --print-config        # show the effective config and paths
kugou-tui --tick-ms 1000        # power-saving: idle CPU near zero
kugou-tui -s "海阔天空"          # search on startup (login required)
```

### Environment variables with no CLI flag

These are environment-only because they are either diagnostic switches or control where the config file lives — the latter cannot be written inside the config file.

| Variable | Effect |
|---|---|
| `KUGOU_TUI_CONFIG_DIR` | **Overrides the config root directory.** When set to a path, config is read/written at `<path>/config.toml` instead of `~/.config/kugou-tui`. Useful for portable installs; tests use it to isolate state to a temp directory. The cache directory is unaffected and can still be set separately with `--cache-dir` |
| `KUGOU_TUI_NERD_FONT` | `1` forces icon rendering as if Nerd Font is installed, `0` forces ASCII as if not. Empty means auto-detect (queries `fc-list` on Linux; **Windows has no fontconfig, so auto-detect always assumes not installed**) |
| `KUGOU_TUI_DEBUG` | `1` enables DEBUG-level logging (per-frame logs such as keys and position sync are off by default, otherwise they drown the log) |
| `KUGOU_TUI_MEM_TRACE` | `1` writes RSS and the current track to the log every 5 seconds, for diagnosing memory growth during long listening |
| `KUGOU_API_DIR` | Directory of the local KuGouMusicApi. When set, **no auto-search or download** happens; a wrong path errors out immediately |
| `KUGOU_API_AUTO_START` | `0` / `false` / `no` / `off` disables auto-starting the API service |

```powershell
# Windows: portable directory + enable Nerd Font icons
$env:KUGOU_TUI_CONFIG_DIR = "D:\apps\kugou-tui\config"
$env:KUGOU_TUI_NERD_FONT  = "1"
.\kugou-tui.exe
```

## Config file

Generated automatically on first run. On Linux/macOS it is `~/.config/kugou-tui/config.toml`; on Windows it is `%APPDATA%\kugou-tui\config.toml` (mode `0600` on Unix, since it may contain cookies). Cache and logs live in the cache directory: `~/.cache/kugou-tui/` on Linux, `%LOCALAPPDATA%\kugou-tui\` on Windows.

If unsure which file is actually read, use `--print-config` (it prints the config, log, and cache paths together with the effective values).

```toml
api_backend = "native"              # native (default, embedded) | node (fallback to local service)
api_base = "http://127.0.0.1:3000"  # used only with --api node
cookie = "token=xxx; userid=xxx"   # login state, can be set by hand
dfid = "..."                        # device fingerprint, fetched automatically on first start
volume = 0.7                        # 0.0–1.0
audio_device = "..."                # output device name, empty = follow the system default
playback_mode = "sequential"        # sequential | repeat_all | repeat_one | shuffle
cache_dir = "/home/you/.cache/kugou-tui"
cache_limit_mib = 512               # 0 = unlimited
lyric_offset_ms = 0                 # global lyric offset, positive = delay
lyric_anim_ms = 200                 # max duration of line fade in/out, 0 = off
tick_ms = 200                       # UI refresh interval (ms)
page_size = 30                      # items per page for search results and the playlist square
proxy = "http://127.0.0.1:7890"     # optional
quality = "128"
basic_color = false
theme = "default"                   # see "Themes" below
download_dir = "~/Music"            # single-track downloads go here, may be empty
qr_aspect = 2.0                     # terminal character height:width ratio, see below
lite_mode = false                   # see below
cover_fill = "crop"                 # how the home cover fills its box, see below
tray = true                         # system tray, see below
api_auto_start = true               # auto-start the service when it is not running, see below
api_dir = ""                        # local KuGouMusicApi directory, empty = auto-detect

# ---- Per-source connection and identity, plus the active one ----
# The four sources are independent: login state and device identity **cannot be reused across sources**.
# See the sources section of the user guide.
[sources]
active = "kugou"                   # kugou | kugou_concept | netease | sodam

[sources.kugou]
enabled = true
api_base = "http://127.0.0.1:3000"  # local KuGouMusicApi (standard)
cookie = "token=…; userid=…"        # this source's own login state
device_id = "…"                      # this source's own dfid
priority = 0                         # lower numbers come first

[sources.kugou_concept]
enabled = true
api_base = "http://127.0.0.1:3001"  # local KuGouMusicApi (platform=lite)
priority = 10

[sources.netease]
enabled = true
api_base = "http://127.0.0.1:3002"  # local NeteaseCloudMusicApi
priority = 20

[sources.sodam]
enabled = true
api_base = "https://api.qishui.com" # Sodam connects **directly to the public internet**; no local service needed
cookie = "sessionid_ss=…; sessionid=…"
priority = 30
# Device fingerprint may be empty when using the signer service; only the manual-capture path needs it
# device_id = "…"

# Sodam's signer service: determines whether VIP full tracks and lossless are available.
# Without a signature the server replies "HTTP 200 + empty body", which looks like the membership
# is not active, so this item is worth checking first.
[sources.sodam_app]
# Empty = use the upstream built-in public signer service (works out of the box).
# Replace it with your own, or set "none" to disable signing explicitly.
# signer_url = "http://127.0.0.1:8899/sign"
# signer_token = "only needed if the service enables auth"
# The following are needed only for the **manual capture** path (leave empty when using the signer service):
# device_id / iid / fp empty = use the signer's own device identity
# x_helios / x_medusa are captured static signature headers and will expire
```

### Kugou API backend (`api_backend`)

Default `native`: the standard and concept Kugou APIs are implemented entirely in-process in pure Rust, **no Node.js required**, usable immediately, with no "prepare service on first run" step.

`node` is a fallback: it uses a local (or remote) KuGouMusicApi service, kept for self-hosted services and for diagnosing differences from the embedded implementation. How to switch:

```bash
kugou-tui --api node            # temporarily switch back to the Node service
kugou-tui --api native          # temporarily switch back to embedded (same as default)
```

```toml
api_backend = "node"            # permanent switch, write into the config file
```

Both backends share the same login state and device fingerprint (`cookie` / `dfid`), so **switching back and forth requires no re-login**. This switch only affects the two Kugou sources; Netease and Sodam are unaffected.

### Auto-starting the API service

> **Applies only to the `--api node` fallback (or the Netease source).** The default embedded backend (`--api native`) implements the API in-process and needs neither a local service nor Node.js.

`--api node` uses the third-party KuGouMusicApi service (written in Node.js), which `cargo install` cannot ship, so the program provisions it at **runtime**: it probes the port once on startup, reuses an existing service (and leaves it alone on exit), and only acts if none is found. Lookup order: configured `api_dir` → `/usr/share/kugou-tui/api/kugou` (distro package) → `~/.local/share/kugou-tui/api/kugou` → `~/KuGouMusicApi`; if none exist it downloads the pinned commit and runs `npm install --omit=dev`.

| Scenario | How to |
|---|---|
| Service runs permanently on another machine / managed by systemd | `api_auto_start = false` (or `--no-api-start`) |
| Want to pre-install for instant startup later | `kugou-tui --api node --api-start` (the service stays in the background; stop it with `--api-stop`) |
| Service directory is elsewhere | `api_dir = "/opt/KuGouMusicApi"` or `KUGOU_API_DIR=...` |

**Requirements**: Node.js (≥ 12) and npm on the machine. By default a service started by the program stops when it exits, leaving no resident process. To drop this bootstrap code from the binary, build with `--no-default-features` (see [README](../README.md#install)).

### Cover fill (`cover_fill`)

The cover area is some number of columns × rows, which almost never converts to a square in pixels, while album art is usually square — of "fill", "no distortion", and "no cropping", only two can hold at once:

| Value | Fills | Distorts | Crops | Description |
|---|---|---|---|---|
| `crop` (default) | yes | no | yes | center crop, equivalent to CSS `object-fit: cover` |
| `stretch` | yes | yes | no | stretches to the area size; square art gets widened horizontally |
| `fit` | no | no | no | image stays whole, but the background shows on the sides when the box is wider than the image |

Toggle at runtime from the settings page (`,`); changes take effect immediately and are written to disk.

### Lyric animation (`lyric_anim_ms`)

Maximum duration of the fade in/out when the line changes (default `200`, `0` = off).

A terminal has no sub-cell positioning — one character cell is one line and cannot move — so a "scroll up" can only jump whole lines; the only dimension that can actually transition is **color**: at the switch the old line is still lit and the new one is not, then they cross-fade, while the brightness levels of the other lines smoothly re-flow.

The actual duration adapts to line spacing: `min(this value, interval from this line to the next × 0.55)` — lines in fast songs last only a few hundred ms, so using the cap would feel sluggish. Three cases **force the transition off**:

| Case | Reason |
|---|---|
| `lite_mode = true` | its selling point is fewer redraws |
| `basic_color = true` | 16 colors have no intermediate levels; the fade degrades to a hard flip past the halfway point, which looks worse than no animation |
| Spanning 3+ lines at once | that is dragging the progress bar or clicking a lyric line to jump; fading line by line is slow and disorienting |

### Output device (`audio_device`)

Default is to **follow the system default** (this field empty). The last item in the settings page shows the **card actually opened**, and you can switch between "system default" and the enumerated devices.

It exists because of a typical Linux failure: the progress bar moves and the state is "playing" but there is no sound — audio went to a different card (e.g. the onboard jack with no speakers), and **directly to the hardware** (bypassing PipeWire), so `pactl list sink-inputs` does not even show this program.

The root cause is usually outside the program: ALSA's `default` is pinned to a card by `/etc/asound.conf` or `~/.asoundrc` (`pcm.!default { type hw card 2 }`). **Card numbers change** (plugging in a USB sound card or a different boot order makes "card 2" point elsewhere). The fix is to point `default` back at PipeWire (`type pipewire`), or to delete that file. Three diagnostic steps:

```bash
ls -l /proc/<pid>/fd | grep snd      # /dev/snd/pcmCxD0p = direct hardware (bypasses PipeWire)
pactl list sink-inputs               # while playing, your stream should appear here
aplay -D default /dev/zero -f cd     # failing to open (busy / no such device) means the ALSA config is broken
```

The candidate list is filtered: ALSA reports every PCM it defines as a device (52 on the author's machine, mostly `lavrate` / `samplerate` / `jack` / `oss` plugins), but only those that "provide a default output configuration" and are not `null` are kept, then de-duplicated by name. Selecting a device that fails to open does not silence playback — it falls back to the previous card and only shows a status-bar notice.

### Other settings

- **`qr_aspect`** — shape correction for the login QR code. When a terminal character is about twice as tall as it is wide, half-block art comes out square. If the QR code looks stretched or squashed, press `L` to show it and measure the height:width ratio of one character: `< 1.5` uses full blocks, `>= 1.5` uses half blocks.
- **`theme`** — `default` (cool blue), `graphite`, `sunset`, `forest`, `neon`, `dracula`. A typo or removal falls back to `default`. Switch it from the settings page (`,`).
- **`quality`** — `128` (default), `320`, `flac`, `high`, `super`, `viper_clear`, `viper_atmos`, `viper_tape`. The last three are "Viper sound effects", **supported only for some songs**; when unavailable the server returns an empty URL and the UI shows a notice.
- **`lite_mode`** — disables the three most resource-hungry features (**playback itself is unaffected**): no cover download/decode, no real-time spectrum, refresh drops to 5 fps and no animation speedup. Measured resident memory: 19.1 MiB idle / 19.1 MiB playing (`--api native`), and enabling this lowers it further. Per-scenario numbers in [DESIGN.md](DESIGN.md#low-resource-footprint).
- **`tray`** — system tray, on by default: after registering as `org.kde.StatusNotifierItem`, Quickshell / waybar / KDE show an icon whose right-click menu offers play/pause, previous, next (plus "minimize / show window" under niri). It is skipped automatically when there is no graphical session, no session bus, or no `StatusNotifierWatcher` from the status bar (each logged once). **Changes take effect after a restart.** On non-Unix platforms the code is not compiled and the value defaults to `false`.

Volume, playback mode, and lyric offset are written back automatically on exit.

### Custom keybindings

Map **action names** to **keys** in the `[keymap]` section. Action names are the snake_case form of `Action` variants:

```toml
[keymap]
reload = "f"                 # move refresh-current-list (default R) to f
help = "f1"
quit = "Z"
cycle_artist_filter = "ctrl+f"
```

**Actions not listed keep their default keys**, so old config files need no changes; write only the entries you want to change.

| Key syntax | Examples |
|---|---|
| Single character (**case-sensitive**) | `q`, `Q`, `/`, `[` |
| Named key | `space`, `enter`, `esc`, `tab`, `backtab`, `up`, `down`, `left`, `right`, `home`, `end`, `pgup`, `pgdn`, `backspace`, `delete`, `insert` |
| Function key | `f1` – `f12` |
| With modifiers | `ctrl+n`, `alt+1`, `shift+tab` |

Available action names, grouped by purpose:

```
# global
quit  force_quit  help  toggle_sidebar  switch_source
# navigation
move_up  move_down  move_top  move_bottom  page_up  page_down
focus_next  focus_prev  submit  cancel  context_menu
backspace  delete  cursor_left  cursor_right  cursor_home  cursor_end
# playback
play_pause  next  prev  seek_forward  seek_backward
volume_up  volume_down  toggle_mute
cycle_playback_mode  cycle_quality
toggle_lyric_panel  lyric_delay  lyric_advance
# business
open_search  open_settings  reload  load_more_search  download_current
queue_append  queue_play_next  add_all_to_queue  remove_from_queue  clear_queue
clear_cache  toggle_sort_order  open_ranks  open_cloud
login  claim_vip  cycle_artist_filter
add_to_cloud  remove_from_cloud  sync_to_cloud  delete_cloud_playlist  new_cloud_playlist
set_default_source  raise_source_priority  lower_source_priority
# window (niri only)
toggle_window
```

> `toggle_window` is the **only action with no default key**: it is normally clicked from the tray menu (right-click → "minimize / show window"). It works only under niri (the test is whether `NIRI_SOCKET` exists) — Wayland's xdg-shell has no "minimize" request, so a terminal program must go through the compositor's IPC.
>
> Actions with parameters (switching tabs, seeking to a given position, typing characters) are not in the list — their values come from runtime and cannot be expressed fully in the config file.

**Invalid entries are skipped without aborting startup**, and a WARN is logged (`~/.cache/kugou-tui/kugou-tui.log`):

| Case | Log |
|---|---|
| Unknown action name | `键位配置：未知动作 "xxx"（键 "yyy"），已忽略` |
| Unparsable key name | `键位配置：无法解析按键 "yyy"（动作 xxx），已忽略` |
| One key bound to two actions | `键位配置：… 同时绑定了 …，以 … 为准` |

On successful startup there is a line `已加载 N 条自定义键位`; use it to verify the count.

### Session persistence

On exit, the **play queue + current track + playback position** are saved to `~/.cache/kugou-tui/session.json`; the next start restores them and shows "已恢复上次会话：N 首 · 按 Space 继续播放".

> This path is **fixed**; `--cache-dir` / `cache_dir` only change the audio cache directory and do not move the session file with it (same for logs). To move the session too, change `XDG_CACHE_HOME`.

**Auto-play is deliberately disabled** (making noise the moment the program opens is startling). After restoring, press `Space` to continue, and playback **resumes from the previous position**, not from the start. That position applies only to the restored track: playing something else first discards it — otherwise pressing play on any track next time would start from the old position.

The same file also records `vip_claimed_day` (the date the concept-edition daily VIP was last claimed); it is what enforces "claim once per day" (that endpoint has risk controls). Deleting the file makes the program claim again on the next start, usually harmless.
