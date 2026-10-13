# Keybindings and Controls

### Shortcuts

Keys have two modes. **When an input box has focus** (search box, new-playlist input), letter keys insert text instead of triggering shortcuts — otherwise you couldn't search for songs with `q` or `j` in the name. Press `Esc` to leave input mode.

**Global and Navigation**

| Key | Action |
|---|---|
| `q` / `Q` | Quit / force quit |
| `?` / `F1` | Help panel (38 entries don't fit, see below) |
| `\` | Show / hide sidebar |
| `1`–`9`, `0` | Switch tabs |
| `,` | Open settings page (same as `9`) |
| `v` | Open sources page (same as `8`) |
| `Tab` / `Shift+Tab` | Rotate focus between panes |
| `j` / `k` or `↓` / `↑` | Move up / down |
| `g` / `G` or `Home` / `End` | Jump to first / last row |
| `PgUp` / `PgDn` | Page up / down |
| `Enter` | Play selected song / enter |
| `Esc` | Go back one level |

> Number-key targets: `1` Home, `2` Search, `3` Playlists, `4` Artists, `5` Charts, `6` Cloud, `7` Queue, `8` Sources, `9` Settings, `0` Visualization.
>
> The sidebar's **display order** differs (the "Now Playing" group comes first; Home is the first sidebar item), so each row is prefixed with its number key — follow those and you can't go wrong. In the sidebar, `↑` `↓` / `j` `k` follow **display order**, matching what you see on screen.

**Playback**

| Key | Action |
|---|---|
| `Space` | Play / pause |
| `n` / `p` | Next / previous track |
| `←` / `→` | Rewind / forward 5 seconds |
| `+` / `-` | Volume ±5% |
| `m` | Toggle mute |
| `r` | Loop mode (order → list loop → single loop → shuffle) |
| `l` | Toggle lyrics panel |
| `[` / `]` | Delay / advance lyrics by 100 ms |
| `y` | Switch quality (takes effect on next track) |
| `W` | Download current track to the directory set in settings |

**Queue and Cloud**

| Key | Action |
|---|---|
| `;` | Open the **song context menu** (or right-click) |
| `a` / `i` | Add to queue / insert next |
| `A` | Add the **entire** current list to the queue |
| `x` / `X` | Remove selected from queue / clear queue (needs confirm) |
| `o` | Ascending ↔ descending (default descending, last track on top) |
| `s` / `S` | Favorite track to cloud / sync entire queue |
| `d` / `D` | Remove from cloud playlist / delete playlist (needs confirm) |
| `N` | New cloud playlist |
| `L` | QR login |
| `V` | Claim today's Concept Edition VIP (Concept source only, see below) |
| `R` | Refresh current list |
| `/` | Focus search box |
| `b` / `c` | Charts / cloud playlists |
| `f` | Artist region filter (on artist page) |
| `Enter` / `E` / `K` / `J` | Sources page: enable·disable / set default / priority up / down (on sources page) |
| `C` | Clear audio cache (needs confirm) |
| `M` | Load more search results (append next page) |

> `x` / `X` act on the play queue — focus the queue pane first (rotate with `Tab`).
> `d` / `D` act on cloud playlists — select a playlist on the "Cloud" tab first.
>
> The four sources-page keys work only on that page (reach it with `8` or `v`); the footer prints the same hint. **Changing priority decides the order of multiple source results within one search**; `E` sets the selected source as the default on startup.
>
> `V` claims the **KuGou Concept Edition** daily VIP (Concept Edition provides this mechanism). When the current source is Standard, it only prompts you to switch with `v` — that's a Concept-exclusive endpoint a Standard account can't call. It **claims once automatically** after login and on switching to Concept Edition, once per day; `V` is the manual retry, and the row in "My Profile" is also clickable.

### Mouse Actions

| Action | Effect |
|---|---|
| Click a list row | Select that row and focus its pane |
| Double-click a list row | Activate (same as `Enter`) |
| **Right-click a list row** | Open the **song menu** (same as `;`), see below |
| Scroll wheel | Move selection in the list under the cursor |
| Click a sidebar tab | Switch tab (hit by **display order**, independent of number keys) |
| Click the progress bar | Seek to that time |
| **Click a lyric line** | **Jump to that line's start time** (translation/transliteration lines jump to their line too) |
| Click a setting item | Select; **click again** to change the value |

> Clicking a lyric line to jump **includes the lyric offset**: the current line is computed as "playback position − `lyric_offset_ms`", so the jump target is `line time + lyric_offset_ms`. Invisible at offset 0, but anyone who has adjusted the offset will immediately notice the jump landing exactly one line off.

### Song Context Menu

Press `;` on the song under the cursor, or **right-click**:

```
╭──《稻香》─────────────────╮
│ > Play              Enter │
│   Add to queue          a │
│   Insert next           i │
│   Favorite to cloud     s │
│   Download locally      W │
╰───────────────────────────╯
```

Every item shows its corresponding key — the menu only gathers the keys at the cursor, it is **not a separate interaction**, so it never contains an action with no key. In the play queue, "Add to queue" becomes "Remove from queue" (adding a track already in the queue again is pointless).

`↑` `↓` to select, `Enter` to execute, `1`-`5` to select directly, `Esc` to close.

### Help Panel

Opened with `?`. There are 38 shortcuts total; a 34-line terminal fits only 26 at a time, so the panel scrolls:

| Key | Action |
|---|---|
| `j` / `k`, `↑` / `↓` | Scroll one line up / down |
| `PgUp` / `PgDn` | Page (step = visible lines − 1, one line of overlap as an anchor) |
| `g` / `G` | Jump to first / last entry |
| `?` / `Esc` | Close |

The bottom shows "entry N-M / 38 total"; this line is hidden when everything fits. The panel is modal: apart from scrolling and closing, all other keys are swallowed, so you can't accidentally operate the list underneath.
