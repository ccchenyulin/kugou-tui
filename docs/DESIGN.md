# Design Notes

## Thread Model

```
input thread ──┐
               ├─→ EventBus ─→ main loop ─→ draw (60 fps cap, event-driven in practice)
async tasks ───┘                    │
                                    └─→ update AppState
audio thread ───────────────────────→ atomics (position/duration/volume)
```

- **Input thread** only does `event::read()`, pushes key and mouse events into a channel, and makes no business decisions.
- **Main thread** is the only place that mutates state, so no locks are needed.
- **Playback position, duration, volume** are shared via atomics: the main thread reads them directly, zero allocation, zero locks.
- **Tokio spawns only 2 workers**: the concurrency ceiling is exactly the "search + lyrics + fetch URL + download" chains.

## Low Resource Footprint

Measured (release build, CachyOS / Linux 7.2 x86_64, **112×34 terminal**, `--no-tray`, reading `VmRSS` from `/proc/<pid>/status`; `--api native`, i.e. the interface runs in-process with no Node on the machine):

| Scenario | Resident memory |
|---|---|
| Idle after startup (not logged in) | 19.1 MiB |
| After search | 19.1 MiB |
| During playback | 19.1 MiB |
| Logged in (avatar, membership info, cover all loaded) | 21.5 MiB |

> The difference between logged out and logged in (19.1 vs 21.5) comes from **account data itself**: the avatar is a decoded bitmap, and the membership summary and listening time also take space. So what "idle" costs depends on whether you are logged in and whether a cover is loaded.
>
> Three repeated measurements (logged out, idle): 18.98 / 19.02 / 19.07 MiB, jitter about 0.1 MiB.

**Comparison with before the migration (`origin/main`, Node backend)**—same machine, same config, same 112×34 and `--no-tray`, only the binary changed:

| | Logged-out idle | Logged-in idle | Threads |
|---|---|---|---|
| Before (`origin/main`, via local Node service) | 18.6 MiB | 20.2 MiB | 11 |
| Now (`--api native`) | 19.1 MiB | 21.5 MiB | 12 |

**The program itself grows by only about 0.3–0.4 MiB** (a 12-vs-11 thread difference plus the resident cost of the embedded HTTP/TLS client and KRC decryption). The real gain is **outside the process**: before the migration a Node service process had to stay resident, and that process alone takes—

| Local service | RSS |
|---|---|
| `node app.js --port=3000` (standard source) | 42.5 MiB |
| `node app.js --port=3001` (concept source) | 66.9 MiB |

`--api native` removes that entire process: no more node in `ps`, and the program no longer spawns any child processes (measured: empty child process list, `node app.js` process count unchanged before and after). So counting "how much the whole player takes", it drops from **60–86 MiB to about 19–21.5 MiB**; counting "how much the TUI process itself takes", it is 0.3–0.4 MiB more than before the migration.

> **Measurement note**: these are RSS (`VmRSS`) figures—the total resident physical pages mapped by the process, including read-only pages shared with other processes (double-counted when the same binary runs multiple times). By Pss (shared pages apportioned), logged-out idle is 12.5 MiB before and 12.9 MiB now; logged-in now is 14.6 MiB. **Both measures agree: the program itself changes by less than 0.5 MiB, and the gain comes from removing the Node process.**

**The table above only holds when "playing small tracks".** The streaming byte buffer **does not keep the whole track**: every downloaded byte is written to disk first, and memory keeps only a tail window near the read pointer (4 MiB cap, see "Streaming Playback" below). So resident memory is independent of track size—playing a 65 MiB Hi-Res track still costs those few MiB. Measured:

| Scenario | Resident memory |
|---|---|
| Playing a 64 MiB track (before the fix, whole buffer kept in memory) | 66.5 MiB |
| Same (now) | 12.5 MiB |
| Switching through 5 tracks, each 64 MiB, none of the downloads cancelled (before the fix) | 323.7 MiB |
| Same (now, switching cancels the download) | 3.0 MiB |

The numbers come from comparing the same workload in `streaming.rs` across **the old and new implementations** (single-track and track-switching scenarios, reading `VmRSS`/`VmHWM` from `/proc/<pid>/status`).

**Binary size** (release, strip, `lto = "fat"`, `codegen-units = 1`):

| Build | Size |
|---|---|
| Before the migration (`origin/main`) | 11,836,312 bytes (11.29 MiB) |
| Now, default features | 12,261,728 bytes (11.69 MiB) |
| Now, `--no-default-features` (without Node bootstrap code) | 12,195,608 bytes (11.63 MiB) |

The migration adds a net of about **0.41 MiB**, from the embedded signing/encryption/KRC/HTTP stack; turning off `node-bootstrap` saves only 0.06 MiB, because the bootstrap code is small (mostly string constants and a few `Command` calls).

Main trade-offs: event-driven rather than busy polling (main loop `recv_timeout(tick)`, 200 ms default); audio fully written to disk; download progress reported only every 256 KiB; no `tracing` / `chrono` / `rand`, replacing their respective dependency trees with a few dozen lines of code.

## Streaming Playback

After fetching the playback direct URL, **it starts playing once the beginning has been buffered**, and the rest keeps downloading in the background while being cached to disk. So the wait for first playback is "buffer the beginning" rather than "download the whole track"; a second play of the same track hits the disk cache directly, with zero wait.

The start threshold is `PREROLL_BYTES = 128 KB` (`src/audio/download.rs`). This number is derived backward from the wait time: at a common bitrate of 128 kbps ≈ 16 KB/s, 128 KB is about 8 seconds of audio—enough for the decoder to run stably, while the wait stays unnoticeable. Too small and it stutters repeatedly; too large and it falls back to "play only after downloading everything".

### Why a Custom Buffer

`rodio::Decoder<R>` requires `R: Read + Seek` (rodio 0.22 has no `new_stream`), while a network stream can only `Read`, not `Seek`, so it cannot be fed in directly.

`StreamingBuffer` (`src/audio/streaming.rs`) accumulates a **growing** byte buffer in memory (but keeps only a tail window, see below) and implements `Seek` on it: when the read pointer runs past what has been downloaded, it **blocks and waits** for the background download thread to supply more data. The decoder therefore thinks it is reading a "very large local file" while the data is still in flight. The behavior is—the sound pauses, rather than ending early.

Blocking has a cost: while `read()` blocks, rodio's playback thread stops there. So every wait carries a **15-second timeout**—a genuinely dead network must be able to error out and exit, not hang forever.

### Memory Cost: Keep a Window, Read the Rest Back from Disk

The buffer **keeps only a tail window** (`MAX_KEPT_BYTES = 4 MiB`, recycled down to 1 MiB). The download thread **writes each chunk to the `.part` file first, then pushes it into the buffer**, so bytes outside the window can be `pread` back from that file at any time (the data has just hit disk, so it is in the page cache, costing close to a memcpy). Thus:

* resident memory is decoupled from track size (see the table above: a 64 MiB track takes 12.5 MiB, not 66.5 MiB);
* seeking backward into an already reclaimed region still works—that part is on disk, no error is reported.

Why reclaiming is safe: **the data is already on disk**; reclaiming discards only the in-memory copy. Not reclaiming also works, but then playing Hi-Res would make RSS grow linearly with track size (measured 1:1), directly conflicting with the "stay within 20 MiB resident" positioning.

The on-disk file is read through an **already-open file handle**, not by path: after the download completes the file is renamed to the official cache file, and cache eviction may delete it, but as long as the handle is alive the inode is alive and the data remains readable.

### When the Stream Breaks

A read timeout, a download failure, or a cancellation—all three starve the data supply. The buffer only records the state faithfully (`is_finished` / `error` / `is_cancelled`) and **does not itself decide "this track is finished"**—that is the playback engine's job: rodio swallows any read error as EOF (symphonia's `format.next_packet().ok()?`), and "end of source" may be either a real finish or a broken stream. The concrete consequence of conflating them: single-track repeat (or a queue with only one track) **plays it again from the start**, and what the user sees is "the progress jumped back to the beginning".

So when the player goes empty, the engine settles in four ways depending on the stream state:

| Stream state | Outcome |
|---|---|
| Actively cancelled (user switches track / stops) | Silent, nothing reported |
| Fully downloaded | `TrackFinished`, the main thread advances normally |
| Not finished, has an error | `Failed(reason)`: report the error faithfully, **do not switch track, do not replay** |
| Not finished, no error (read timeout) | `StreamInterrupted { position }`: **retain the position**, the main thread resumes once from that position |

The position must be recorded **before the track ends**: once the source ends, `Player::get_pos()` reports 0, and reading it on that frame erases the position the user heard (`Runtime::last_position_ms` exists for this).

### Write to `.part`, Not the Final Name

Both download paths write to a `.part` in the same directory first and **rename only after everything is written**. Streaming playback needs this especially: it plays while downloading, and failure/cancellation mid-way is the norm. Writing directly to the final name would leave a file on disk that "looks complete but is only half a track", while `cache.find` decides purely by "does the file exist"—the next play would hit it and end inexplicably in the middle, and under single-track repeat it would restart from the beginning over and over.

Cache eviction and "clear cache" both skip `.part` (it is not a cache, it is a half-finished product); a `.part` left by a crash is `truncate`d and reused the next time that track is played.

## Interface Adaptation

KuGouMusicApi is a reverse-engineered wrapper, its response structure drifts, and **the upstream docs disagree with actual behavior** in quite a few places. This project's approach is "multiple candidate key names + tolerant types + a single failure does not take down the whole page". The following are places confirmed against the real service to differ from the docs:

| Upstream docs say | Actual |
|---|---|
| `/login/qr/create` returns `qrcode` / `qrimg` | returns `data.url` / `data.base64` |
| Playlist plaza is in `data.info` | in `data.special_list` |
| Singer list elements contain `songcount` | that field is always `0`, unusable for display |
| Singer list is a flat array | actually nested in `data.info[].singer[]` |
| `/song/url` returns `data.url` | actually at the **top level** `url` / `backupUrl` (arrays), `data` does not exist |
| Docs suggest `npm run dev` | `npm start` is what runs `node app.js` |

The same track uses **three completely different field names** across the search, playlist, and rank interfaces—the most labor-intensive part of the parsing layer, which is why there are more unit tests:

| | Search `/search` | Playlist `/playlist/track/all` | Rank `/rank/audio` |
|---|---|---|---|
| Title | `SongName` | `name` (with "Singer - " prefix) | `songname` |
| hash | `FileHash` | `hash` | none at top level, in `audio_info.hash_128` |
| Duration | `Duration` (seconds) | `timelen` (milliseconds) | `audio_info.duration_128` |
| Album | `AlbumName` | `albuminfo.name` | `album_info.album_name` |
| Singer | `Singer[]` | `singerinfo[]` | `authors[]` / `author_name` |

```bash
cargo test     # covers the parsing paths above, plus edge cases for queue / cache / key handling
```

## Retry

Transient failures on read interfaces are retried automatically; write interfaces are not retried. The sole criterion is: **could the same request produce a different result if sent again** (`AppError::is_transient`).

| Failure | Retry | Why |
|---|---|---|
| Connection refused / connection reset / response body truncated mid-read | Yes | Returns in milliseconds, exactly the "network works but hiccuped" shape |
| HTTP 408 / 429 / 5xx | Yes | The peer explicitly says "not now, come back later" |
| **Timeout** | No | Already waited the full 15 seconds (8-second connect timeout), meaning the peer is stuck, not hiccuping; two more rounds trade the user's time for a most likely identical result |
| Business error code (`error_code`) | No | The service responded, it just rejected this request; resending gets the same answer |
| Other 4xx | No | The request itself is wrong (parameters / auth), unrelated to timing |
| 200 but non-JSON body | No | A content problem; a real truncation goes through a transport error, which is retryable |
| Write operations (favorite / remove / delete playlist / create playlist / claim VIP) | No | Resending may change the result: if `/playlist/del` actually succeeded the first time and only the response was lost, resending yields "playlist does not exist" |

Count and interval: **3 attempts total (first + 2 retries), interval 300ms → 900ms** (×3 increase, no jitter—a single-user local client has no "many clients retrying at once" problem). So one hiccup costs the user at most 1.2 extra seconds; more than that is not "riding out a hiccup" but "stalling instead of reporting an error".

> Why it is worth doing: the path that fetches the playback URL tries multiple candidate (hash, quality) pairs one by one; a candidate skipped because of a single hiccup—after all of them hiccup—leaves the user seeing "no available playback URL (may require VIP or has been removed)", **a conclusion entirely unrelated to the real cause**.

---
