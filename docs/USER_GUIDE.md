# User Guide

## Features

| Category | Capability |
|---|---|
| Browse | Playlist plaza, artist list (filterable by region), rankings, personal cloud playlists |
| Search | Single-track search (available after login); `M` loads more |
| Playback | Play / pause / previous / next / seek (±5 s) / volume (±5%) / mute |
| Playback modes | Sequential, repeat-all, repeat-one, shuffle |
| Lyrics | Per-character (KRC timestamps), translations / transliterations, centered scrolling, line cross-fade, click a line to seek, ±100 ms offset |
| Visualizer | **Real spectrum**: FFT over samples captured on the audio thread, log-spaced (40 Hz–16 kHz); bass lights the left, cymbals the right |
| Queue | Append (`a`), play next (`i`), add whole list (`A`), remove (`x`), clear (`X`) |
| Cloud playlists | Favorite (`s`), sync whole queue (`S`), remove (`d`), delete playlist (`D`), create (`N`) |
| Login | **In-app QR scan** (`L`); the QR code is drawn in the terminal |
| Concept-edition VIP | **Claimed automatically each day** (attempted at startup / after login / on switching to the concept edition; once per day); `V` retries manually |
| Track download | `W` downloads the current track to the directory set in settings (default `~/Music`) |
| Cache | Audio cached to disk with LRU eviction; `C` clears it |
| Network | **Automatic retry on transient failures**: connection refused / reset / truncated body / 408 / 429 / 5xx are retried (3 attempts total, 300 ms → 900 ms apart); timeouts, business error codes, and other 4xx are not. Writes are never retried |
| Input | Keyboard + mouse (click to select, double-click to activate, wheel, click the progress bar to seek) |
| Appearance | 6 themes (cool blue / graphite / sunset / forest / neon / dark purple), each in truecolor and 16-color variants |
| Settings page | Opened with `,`: theme, quality, playback mode, refresh interval, lyric offset and animation, page size, cache limit, 16-color, lyric panel, side navigation, download directory, cover fill, output device; saved immediately on change |
| Desktop integration | **MPRIS** (status bar / media controls / `playerctl`, with cover art) and **system tray** (right-click menu, wheel volume, icon dims with state); Linux desktops only |

Supported audio formats are determined by rodio: **MP3, FLAC, M4A/MP4, OGG (Vorbis), WAV**.

## Interface

```
┌──────────┬──────────────────────────────────┐
│ Sidebar  │ Primary (search box / list)      │
│ grouped  ├────────────────────┬─────────────┤
│ nav      │ Secondary (tracks) │ Lyrics      │
│ + status ├────────────────────┴─────────────┤
│          │ Playback queue                   │
├──────────┴──────────────────────────────────┤
│          │ Track · Artist · Album  Next: ...│
│          │ ━━━━━━━━━━━━━━━━━━━━ 1:23 / 4:56 │
└─────────────────────────────────────────────┘
```

`Tab` / `Shift+Tab` cycle focus between "sidebar → Primary → Secondary → queue"; the focused panel's border is highlighted.

**Number keys have fixed targets**: `1` home, `2` search, `3` playlists, `4` artists, `5` rankings, `6` cloud, `7` queue, `8` sources, `9` settings, `0` visualizer. The sidebar groups items as "Now Playing / Discover / Mine / Settings", **in a different order from the number keys** (a more scannable order), so each row is labeled with its key; inside the sidebar, `↑` `↓` follow the display order.

Below the navigation are two status blocks: **connection** (API address, source, login, fingerprint, membership) and **cache** (used, limit, clear entry). Mode and volume already appear in the playback bar and queue title, so they are not repeated. On short terminals these collapse: the whole block yields to a single condensed status line (e.g. `● embedded · kugou`) rather than being silently clipped.

The **playback bar** is only 4 rows: track info + progress bar, with the leading icon and color showing playback state and the right side showing next / volume / loop mode. It deliberately has **no cover art** (an 8×4-cell album image is just a blur); the cover appears only in the **home** left column, where there is room.

The **home** left column is "cover + my profile". How the cover fills its area is set by `cover_fill` (the terminal cover area is almost never square, while album art usually is) — see [CONFIGURATION.md](CONFIGURATION.md#cover-fill-cover_fill). Profile data comes from `/user/detail`: nickname, avatar, level, cumulative listening time (deliberately excluding follower counts and badges, which would turn it into a data table). The avatar cell draws a real avatar on terminals supporting kitty / iTerm2 / sixel, and the **nickname's first character** on terminals limited to half-block characters (in half-block mode that cell is only 24×12 "pixels"; a photo would be noise). The three "no profile" states are shown distinctly: not logged in, loading, and failure reason + "click to retry".

**Fonts**: icons prefer Nerd Font, probed once at startup with `fc-list`; without it the program falls back to ASCII symbols instead of showing a screen of tofu.

### Daily concept-edition VIP

The concept edition has a built-in "claim one day of VIP per day" mechanism, made automatic here: attempted **after a successful login**, **on switching to the concept edition**, and **at startup**, once per day (the date is recorded in the session file). Claiming also upgrades to "unlimited-listening VIP" — one day alone has limited benefits.

When the active source is the concept edition, an extra status line appears in "My Profile":

```
│ Concept SVIP · until 09-24 │
│ Claim today's VIP · press V │     # claiming… / today's VIP already claimed
```

Press `V` or **click this row** to claim. Under the standard edition this row is not shown (it is a concept-edition-only interface; a dead entry point is worse than none), and pressing `V` prompts you to press `v` to switch first.

> The interface is rate-limited; on repeated failures the prompt tells you to claim on mobile. The official docs also warn against frequent calls, so this is strictly de-duplicated per day.

### Sources (Kugou / Kugou concept edition)

Press **`v`** to cycle through **enabled** sources (in the priority order set on the source management page); the choice is written to the config file. Switching **does not interrupt current playback** — it only changes the request address and identity; the playback queue and current track are untouched. `--print-config` tells you the server-side config it needs (e.g. `active source : Kugou concept edition (server needs platform=lite)`).

The two sources have **independent login state and device identifiers** (the tokens for the two Kugou platforms are not interchangeable); a QR code scanned on source A does not carry over to B. If the switched-to source is not logged in, the UI prompts you to press `L` to scan again.

> The "concept edition" is not a skin but a separate membership and authentication system. A membership belongs to a platform, so you must use that platform's instance and **log in on that platform**, otherwise it degrades to preview.

> Soda's QR scan uses the CDP signer page built into libresoda; press `L` and select it — see the "Soda Music source" section.

### Netease source

Netease goes through **NeteaseCloudMusicApi**, a separate service from Kugou's KuGouMusicApi. The program only knows the single `api_base` address and **is not bound to a specific implementation** — the original Binaryify one, api-enhanced, or any interface-compatible fork works:

```bash
# Or do it in one step: ./scripts/kugou-api-install netease
git clone --depth 1 https://github.com/neteasecloudmusicapienhanced/api-enhanced.git NeteaseCloudMusicApi
cd NeteaseCloudMusicApi && npm install
env PORT=3002 node app.js          # ⚠️ only reads the PORT env var, not --port=
```

```toml
[sources.netease]
enabled = true
api_base = "http://127.0.0.1:3002"
```

**You must then scan the QR code with `L`** — without login, cloud interfaces return "login required". After a successful scan the server issues a cookie (containing `MUSIC_U`), which the client writes to `[sources.netease].cookie` and sends automatically on later startups.

> ⚠️ The server issues a **whole `Set-Cookie`** (with `Max-Age` / `Expires` / `Path`, multiple cookies joined by `;;`), but reads it back recognizing **only the "semicolon + space" separator** — echoing it verbatim folds `MUSIC_U` into the preceding attribute segment, and the UI says "login successful" while cloud playlists forever report "not logged in". The client normalizes it to `k=v; k=v` before storing; **when filling the cookie in by hand, use the same format** — do not paste a whole `Set-Cookie`.

> **Do not** press `L` repeatedly on mobile within a short time to fetch new QR codes — Netease flags "frequent login" and the mobile app cannot scan at all. The program requests a new session on each `L` by default, and blocks presses within 30 seconds with a remaining-wait prompt.

Stream resolution is not login-gated: you can search and play without login, only VIP-exclusive tracks are unavailable. Write operations use the song id (not Kugou playlist entries' `fileid`), so you can favorite directly from search results.

Config is stored per source:

```toml
[sources]
active = "kugou"              # or "kugou_concept" / "netease"

[sources.kugou]
api_base = "http://127.0.0.1:3000"
cookie = "…"                  # this source's own login state
device_id = "…"               # this source's own dfid

[sources.kugou_concept]
api_base = "http://127.0.0.1:3001"
```

### Soda Music source

Soda (Soda Music) data is provided by **libresoda** — the same library used by the official SodaM client ([sodahub-org/sodam](https://github.com/sodahub-org/sodam)). Login, stream resolution, decryption, playlists, artists, albums, and lyrics are all delegated to it, so behavior matches the official client and upstream fixes carry over.

Two fundamental differences from the other three sources, up front:

1. **Connects to the public internet** (`api.qishui.com`); no local service runs. First use needs neither `--api-start` nor Node.js.
2. **The delivered audio is encrypted** (MP4/CENC, key in the stream interface's `play_auth`). Resolving a stream requires "download the whole track → decrypt → write to disk" before handing it to the player, so Soda tracks **cannot be played while downloading**; first play waits for one full download. This is dictated by the encryption scheme, not an implementation shortcut.

#### Minimal config: works with nothing set

```toml
[sources.sodam]
enabled = true
```

Search, per-character lyrics, cover art, and full-track playback of free tracks work anonymously (search goes through an unsigned Android gateway).

#### VIP full tracks and lossless: via the signing service

Soda's stream resolution has two layers; measured behavior is:

| Layer | Endpoint | What you get |
| --- | --- | --- |
| Web / share page | `GET /luna/pc/track_v2` (`device_platform=web`), `/luna/h5/seo_track` | **Only 30 / 60 s preview** |
| App | `POST /luna/pc/track_v2` (with an app-level signature) | **Full track**; without a signature it returns **HTTP 200 + empty body** |

So the root cause of "VIP tracks only play 30 seconds" is **not** that membership failed but that the app-level signature is missing. That empty response is especially counterintuitive — HTTP 200 with nothing in it.

**A public signing service is configured by default** (taken from upstream SodaM's built-in default), so full tracks work out of the box with no packet capture or self-hosting:

```toml
[sources.sodam_app]
# Empty = use the upstream public signing service (default)
# signer_url = "http://your-server:8899/sign"   # switch to your own
# signer_url = "none"                          # disable explicitly; preview / free tracks only
```

> ⚠️ The public signing service is a **shared third-party service**: it may rate-limit, may be temporarily unavailable, and its token may rotate. For long-term use, self-host (see below).

Self-hosting the signing service ([libmssdk](https://github.com/sodahub-org/libmssdk) or libresoda's `tools/qishui-signer-host/signer-service.mjs`):

```toml
[sources.sodam_app]
signer_url = "http://127.0.0.1:8899/sign"
signer_token = "fill this in if your service enables auth"
```

Environment variables work too (same names as upstream, so one set can be shared): `QISHUI_SIGNER_URL` / `QISHUI_SIGNER_TOKEN`.

#### QR login

Press **`L`** and select Soda to scan. It uses the **CDP signer page** built into libresoda — it drives your local Chromium directly, **no Node needed**:

* Prerequisite: Chrome / Chromium / Edge on the machine. Without one it fails explicitly at this step, without affecting search and playback.
* `QISHUI_CHROMIUM_PATH` sets the browser path.
* Each QR code gets its own browser context (the passport service rate-limits by device identity; sharing one gets blocked all the way).
* If a **second verification** (slider / SMS) is required, complete it in the browser window as prompted.

On success you get a **session cookie**, which the program writes to `[sources.sodam].cookie` and sends on later startups.

You can also skip the scan entirely and fill it in by hand:

```toml
[sources.sodam]
cookie = "sessionid_ss=…; sessionid=…"
```

**The device fingerprint can be left empty.** When going through the signing service, the signer has its own device identity; only the manual packet-capture path needs `device_id` / `iid` / `fp` (and they must match the signer).

#### Quality and downgrade

- Soda's quality tiers are decided by the **platform side**; the program picks by "completeness first, then quality": if you ask for lossless but only a 128k **full track** is available, it picks the full track and states "actual quality standard" in the status bar — it will not show flac in the UI while playing 128k with no way to find out why.
- Preview-only tracks also play, but with an explicit "preview clip only" prompt and whether it is due to membership.
- Quality mapping: `flac` → lossless, `super` → highest, `320` → 320k, `128` → 128k; Kugou's `viper_*` tiers have no Soda equivalent and are treated as "unrestricted".

#### Current capability status

| Capability | Status | Notes |
| --- | --- | --- |
| Search | ✅ | Through the unsigned gateway; works without login |
| Playback (incl. full track / lossless) | ✅ | Needs the signing service (configured by default) |
| Per-character lyrics / cover art | ✅ | |
| QR login | ✅ | Built-in CDP signer page; needs local Chromium |
| **Cloud playlists (my playlists)** | ✅ | Requires login; via `/luna/pc/me/playlist` |
| **Playlist detail / tracks** | ✅ | Both own and public playlists open and play |
| **User profile / membership status** | ✅ | Official `/luna/pc/me` gives `is_vip` directly |
| Playlist plaza (recommended / categories) | ❌ | The reference implementation has no such endpoints, only playlist search |
| Artist list (hot artists) | ❌ | Only lookup by id for detail and tracks; no "hot artists" endpoint |
| Rankings | ❌ | No equivalent chart endpoint |

The three ❌ are **conclusions from reading the libresoda code**, not omissions: its `get_recommended_playlists` / `get_playlist_categories` return `Unsupported` directly, and there are no entries for artists and charts. The UI explains this clearly in these three places rather than showing a blank screen that looks like a network problem.

**If you previously saw "this source only supports search and playback (playlists / charts / cloud playlists unavailable)" when switching sources — that message is out of date**; playlists and cloud playlists both work now.

#### The signing service decides which features work

One thing must be clear: cloud playlists, playlist detail, user profile, and membership status all go through the `/luna/pc/` family and **all need the app signature**. So:

- When the default public signing service is available, all the ✅ in the table above work;
- If you set `signer_url` to `none`, or the public signing service is temporarily unavailable, these features fail together (empty responses) — not an account or network problem.

When troubleshooting, first check the "signing service" line of `--print-config`.

#### Cache and decryption intermediates

Decrypted audio goes in the `sodam/` subdirectory of the audio cache directory and does not count toward the cache size limit (it is derived from track id + quality tier and regenerated on next play if deleted).

### Playback queue

- **Order**: playlists and search results default to **descending** (the last track on top); press `o` to switch back to ascending. The underlying data is reversed, so the UI order and playback order always agree.
- **Bulk add**: `A` appends the current list to the queue (hundreds of tracks in one action).
- **Clear**: `X` shows a confirmation first; `Enter` confirms, **stops playback**, and clears. To delete just one, use `x`, which does not interrupt current playback.

### Login and cloud playlists

Press `L` to show a QR code and scan it with the Kugou app. On success the credentials are written to the config file, and **the UI only tells you where they were saved, never echoing the cookie content** (it is equivalent to an account password). You can also pass it manually: `--cookie 'token=xxx; userid=xxx'`. To log out, delete the `cookie` line from the config file.

- When not logged in: browsing and playback all work; search returns `error_code: 152`.
- Cloud-playlist writes (favorite / sync / remove / delete / create) all require login.

**Two write-operation limits** (both inherent to the interfaces):

- `d` (remove from playlist) uses the playlist entry's `fileid`, which only exists for tracks loaded from the playlist interface — so `d` only works on tracks inside a playlist.
- **Rename is not implemented**: `/playlist/update` additionally requires `total_ver` (the playlist version number), which the existing playlist-list interface does not return.

---

## Desktop integration (MPRIS and system tray)

> **Linux desktops only.** Both are D-Bus interfaces: Windows has no session bus, so the related code is not compiled (`--print-config` shows `system tray : unavailable`); on macOS the code compiles, but there is no session bus by default, so it logs one WARN at startup and skips. All other features are identical across the three platforms.
>
> "Minimize / show window" likewise only appears under niri, with its entry in the **tray menu**; to bind it to a key, see [CONFIGURATION.md's custom keybindings](CONFIGURATION.md#custom-keybindings).

At startup it registers `org.mpris.MediaPlayer2.kugou-tui` with the session bus; once registered, the desktop environment **automatically** recognizes it with no configuration:

```bash
playerctl -l                          # should list kugou-tui
playerctl -p kugou-tui status         # Playing / Paused / Stopped
playerctl -p kugou-tui metadata       # title, artist, album, cover URL, duration
playerctl -p kugou-tui play-pause
playerctl -p kugou-tui position 90    # drag the progress bar (via SetPosition)
```

On desktops like Hyprland / DankMaterialShell, the status bar's media controls, notification center, and lock screen show the track and cover directly (`dms ipc call mpris list` confirms whether DMS sees it).

> When multiple players are present (e.g. a browser playing video), each is scored and one is shown. If you do not see kugou-tui, pause the other players first.

### System tray

At startup it also registers an `org.kde.StatusNotifierItem`, and the status bar (Quickshell / waybar / KDE, etc.) shows a music-note icon:

- **Wheel** → vertical adjusts volume, horizontal seeks forward / backward
- **Right-click** → menu: play / pause (worded to match state), previous, next, mute / unmute, minimize / show window (niri only), quit
- **Middle-click** (KDE) or right-click on some hosts → play / pause
- **Hover** → current track; the **icon** is bright while playing / loading and dims while paused / stopped
- State is `Active` while playing and `Passive` while paused

The menu uses `com.canonical.dbusmenu`. **Whether it pops up depends on whether the status bar implements it** — Quickshell's end4 config responds to right-click only when "there is a menu", while the caelestia config only wires up left-click. In the latter case, use the MPRIS controls (the status bar's media controls, `playerctl`), which are functionally identical.

The icon is embedded in the program (source `assets/tray.svg`) and does not depend on the system icon theme. With no graphical session (pure tty, SSH without forwarding) or no session bus, it skips automatically with one log line each. **It is fine if the status bar starts after the player**: the tray thread rechecks the watcher's registration list every 5 seconds and re-registers automatically once the panel comes up, with no need to restart kugou-tui.

To skip the tray, add `--no-tray` or set `tray = false` — **takes effect on restart**.

#### Minimize (niri)

"Minimize / show window" collapses the TUI out of the tiling layout while **music keeps playing**, and clicking again restores it. Once collapsed, the keyboard can no longer reach it, so the tray menu and MPRIS are the only controls.

A terminal program cannot minimize its own window (Wayland's xdg-shell has no such request), so this goes through niri's `toggle-window-minimized`. The program finds its own window by the **terminal title** (set to `kugou-tui` at startup, because the window `pid` niri reports is the terminal emulator's and will not match). Therefore:

- It is offered only when `NIRI_SOCKET` is detected; if you are not on niri, the menu does not have it
- Changing the terminal title another way (e.g. some shells' title templates) breaks this, and the log records "no window titled kugou-tui in the window list"
