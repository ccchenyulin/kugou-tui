# FAQ

## API Service

**Q: Startup says node not found?**
A: That only happens on the `--api node` fallback path — it runs the third-party service KuGouMusicApi, written in Node.js. Install Node.js (≥ 12, with npm) and restart; the program prepares and launches the service itself, no need to clone any repo. **The default embedded backend (`--api native`) does not need Node.js** — if you don't plan to use the fallback, just add `--api native`; to drop the bootstrap code too, build with `--no-default-features`.

**Q: First launch hung for half a minute, is that normal?**
A: Only with `--api node`. The first run downloads KuGouMusicApi (pinned commit) and runs `npm install --omit=dev`, measured at about 25 seconds. After that, each launch reuses the port if found. To do it ahead of time: `kugou-tui --api node --api-start`. The embedded backend has no such step — it's ready on launch.

**Q: Service won't start / port taken?**
A: Check the log `~/.cache/kugou-tui/api-standard.log` (Concept Edition: `api-lite.log`). When the port is held by another program the program won't fight for it — it errors out directly. Use `kugou-tui --print-config` to see which address it probes, or switch with `-a http://127.0.0.1:3100`. To manage the service entirely yourself, set `api_auto_start = false` (or `--no-api-start`).

**Q: How do I configure the Concept Edition source?**
A: The platform is decided by the server-side `platform` environment variable (`lite` = Concept Edition), and one Node process can only load one config, so **Concept Edition needs its own instance**:

```bash
cd KuGouMusicApi && platform=lite PORT=3001 node app.js
# or launch both at once with the repo script: kugou-api start
```

> A program installed via `cargo install` **does not need this done manually**: it automatically brings up that instance when you switch to Concept Edition.

**Q: Why can I still only get previews after switching to "KuGou Concept Edition"?**
A: Most likely you **are not logged in on Concept Edition**. The two platforms' tokens are not interchangeable (upstream docs, verbatim: "不同版本的平台的 token 是不通用的"). A login state scanned on Standard Edition, used on Concept Edition, isn't accepted by the server, membership info is empty, and playback degrades to previews. After switching to Concept Edition, press `L` to **scan the QR code again**. Verification command:

```bash
curl -s -H "Cookie: $(rg '^cookie' ~/.config/kugou-tui/config.toml | sed 's/^cookie = "//; s/"$//')" \
  "http://127.0.0.1:3001/user/vip/detail" | jq '.data'
```

A returned `status: 0` means this instance isn't logged in yet.

**Q: How do I know whether the API service is up?**
A: Look at the title of the "Connection" block in the sidebar — `已连通` / `未连通` / `未验证`. It is driven by **the result of real requests**, not guessed at startup: `未验证` means no request has been sent at this moment, `未连通` means the most recent request was refused or timed out. Showing `未连通` means the service isn't running or the address is wrong (use `--print-config` to see which address it connects to).

> **Business** error codes like "需要登录" / "页码越界" count as **已连通** — the service answered, it just rejected that request.

## Playback

**Q: Can I listen without logging in?**
A: Yes. Browsing the playlist square / artists / charts and playback need no login; only search and cloud playlists do.

**Q: Search does nothing / says login required?**
A: `/search` without auth returns `error_code: 152`; press `L` to scan the QR code. Note that when KuGouMusicApi fails at the business level it puts the error code in the response body while the HTTP status may be `502` — don't assume the service is down on seeing 502.

**Q: Fetching the playback URL fails, saying "本次请求需要验证"?**
A: Usually a missing `dfid`. The program fetches it automatically on first launch; if it's absent from the config, delete the config so it's fetched again. Fetching an audio URL with an MV hash or `hash_multitrack` also reports this, but the normal path uses `audio_info.hash_128` and won't trigger it.

**Q: A song takes a long time to start playing?**
A: Download-and-play is implemented: after getting the direct link it buffers enough of the beginning (128 KB, about 8 seconds of audio) then starts, downloading the rest in the background to disk. So the wait is "buffering the beginning", not "downloading the whole track". To go faster: **lower the quality** (`320` ~10 MB, `flac` ~30 MB, `high` ~65 MB — a physical limit of file size); when the direct link supports Range it **downloads in up to 4 concurrent chunks**; after the current track starts it **prefetches the next queue item**; playing the same track again hits the cache directly.

> Edge case: when the streaming buffer's read pointer reaches a position not yet downloaded it **blocks waiting for data**, showing as sound pausing then resuming (rather than ending early). Audible under heavy network jitter.

**Q: Progress bar moves, status is "playing", but no sound at all?**
A: Press `,` to open the settings page and check the last item "Output Device" — it shows **the card actually opened**, almost certainly not the one you're listening on:

```bash
ls -l /proc/$(pgrep -x kugou-tui)/fd | grep snd   # output = direct hardware, bypassing PipeWire
pactl list sink-inputs                            # not seeing yourself while playing = indeed bypassed
```

The root cause is almost always the system's ALSA config: `/etc/asound.conf` or `~/.asoundrc` hardcodes `default` to some card (like `pcm.!default { type hw card 2 }`), and **card numbers change**. Point `default` back to PipeWire (`type pipewire`, or delete that file). See [CONFIGURATION.md](CONFIGURATION.md#output-device-audio_device).

**Q: Lyrics don't show / are out of sync?**
A: Lyrics come from the API; if there are none, there are none. If only the timing is off, nudge with `[` / `]` in 100 ms steps; the offset is saved to config.

**Q: Memory only grows, never drops, after long listening?**
A: 0.4.7 fixed a round of this (the mmap threshold was set too high, so cover bitmaps fell below it into the brk arena, and pages weren't returned to the OS after free). If it still seems high, launch with `KUGOU_TUI_MEM_TRACE=1 kugou-tui`; the log has one RSS line every 5 seconds along with "streaming task count / queue length / cache capacity" — it distinguishes "something held long-term" from "pages not yet returned". Also `lite_mode` turns off cover decoding, the biggest memory consumer.

## Interface and Data

**Q: Arrow keys / number keys do nothing?**
A: First check whether you're in the search box's input mode — there letter keys insert text, so press `Esc` to leave input mode and try again. If it's confirmed not that, launch with `KUGOU_TUI_DEBUG=1 kugou-tui`; the log records what action each key maps to, telling you whether "the key didn't reach the program" or "it mapped to an empty action".

**Q: The scroll wheel scrolls the page instead of the list?**
A: The program enables mouse capture. If the terminal does extra mouse forwarding (some tmux configs), it may conflict; turn it off with the terminal's own setting.

**Q: Why does opening a playlist with hundreds of songs take a few seconds?**
A: The playlist endpoint **hard-caps at 30 per page** — pass `pagesize` 100 or even 500 and the server still returns 30, and the `count` in the response only echoes the current page's item count, not the total. So the client can only page through: measured, a 385-song playlist takes 13 requests. Charts and artist songs are the same. Search results and the playlist square instead fetch a single page by `page_size` (default 30).

**Q: The count shown for a playlist doesn't match reality?**
A: The playlist metadata's `percount` and `songcount` are measured as always `0` and can't be used for display. The "N songs" in the UI is **the number of items actually fetched**, which is reliable.

**Q: Cache taking too much space?**
A: Default cap is 512 MiB; when exceeded it recycles LRU oldest-to-newest. Change `cache_limit_mib`; `--cache-limit 0` means no limit.

## Platform-Specific

**Q: How do I install on Windows? Why won't `./scripts/kugou-api-install` run?**
A: Those scripts are bash and won't run in PowerShell. On Windows use the corresponding `.ps1`:

```powershell
.\scripts\kugou-api-install.ps1
.\scripts\kugou-tui.ps1
```

Full steps (toolchain prerequisites, terminal requirements, platform differences table) in [INSTALL.md](INSTALL.md#building-and-running-on-windows).

**Q: Windows build reports `linker link.exe not found` / `cmake not found` / `nasm not found`?**
A: The first means the MSVC toolchain isn't installed (install Visual Studio Build Tools and check "Desktop development with C++"). The latter two **should not appear**: on Windows TLS uses the system's built-in SChannel rather than rustls, so that pile of `aws-lc-sys` C code isn't compiled at all — if you actually hit it, you're not building this repo's current code.

**Q: No tray icon / `playerctl` on Windows? Why is the cover a mosaic of color blocks?**
A: Neither is a fault. The tray and MPRIS are D-Bus interfaces; Windows has no session bus, so the relevant code **is not compiled** (`--print-config` prints "系统托盘 : 不可用"). The cover is **half-block character art** — the program detects the terminal doesn't support the Kitty / iTerm2 graphics protocol and degrades automatically, and Windows Terminal supports neither (enable `lite_mode` to skip that overhead). See [INSTALL.md's feature-differences table](INSTALL.md#6-feature-differences-from-linux).

**Q: Icons turned into blocks / question marks?**
A: The terminal font lacks the glyphs. The program first probes for a Nerd Font and falls back to ASCII icons if absent; but **Windows has no fontconfig, so the probe always fails**, and a Nerd Font won't be auto-detected even if installed — set `KUGOU_TUI_NERD_FONT=1` to force it on (on Linux / macOS this variable also overrides the probe result).

**Q: Is macOS supported?**
A: **Code supported, CI covered, but not used long-term on real hardware**. It goes through the same `cfg(unix)` branch as Linux, audio via CoreAudio, build needs only the Xcode command-line tools. Known differences: no MPRIS / system tray, no window minimize (uses niri's compositor IPC), config in `~/Library/Application Support/kugou-tui/`, cache in `~/Library/Caches/kugou-tui/` (**not** `~/.config` and `~/.cache`), and half-block character covers under Terminal.app (iTerm2 can use the graphics protocol). Details in [INSTALL.md](INSTALL.md#building-and-running-on-macos).
