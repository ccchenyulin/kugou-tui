# KAT Reference Scripts (developer-only)

These scripts **are not part of the build** and are not a runtime dependency of the test fixtures. Their sole purpose: from the upstream KuGouMusicApi JavaScript implementation, compute baseline values with fixed inputs, for the known-answer tests in `src/api/native/*.rs` to assert against.

They live here rather than being tossed in `/tmp` because those expected values are **produced by running the upstream source**, not hand-copied constants. When upstream changes an algorithm, or a signature needs re-checking, you must be able to re-run the same baseline and compare; without the scripts, all that's left is a pile of hex strings of unknown provenance.

## Upstream version depended on

```
repo: https://github.com/MakcRe/KuGouMusicApi
version: v1.6.0
commit: a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e
date: 2026-08-14
```

`kat_rsa.js` also depends on upstream `node_modules/node-forge`, so it must run in a clone with dependencies installed (`npm install`).

## Usage

```sh
export KUGOU_UPSTREAM=/path/to/KuGouMusicApi   # default /home/xibie/KuGouMusicApi
node tools/kat/kat_gen.js       # three signature sets, signKey/signParamsKey, raw RSA, calculateMid
node tools/kat/kat_rsa.js       # PKCS1 v1.5 (injected fixed padding), public key n/e/bitLength
node tools/kat/kat_device.js    # randomString / getGuid / calculateMid (injected fixed Math.random)
node tools/kat/kat_notsign.js   # intercept the real song_url / search_lyric options, recompute the signature
node tools/kat/kat_object_order.js  # JSON key order of object-typed param values (basis for preserve_order)
node tools/kat/kat_aes.js       # AES-128-CBC+PKCS7 and the full /register/dev request
node tools/kat/kat_login.js     # full requests for the three QR-login endpoints and the two user-info endpoints
node tools/kat/kat_cloud.js     # full requests for nine read-only endpoints: cloud playlists/artists/charts/membership-claim records
node tools/kat/kat_write.js     # full requests for the four cloud-playlist write endpoints
node tools/kat/kat_request.js       # fixed time and randomness, records the full request built by request.js
node tools/kat/probe_outbound.js    # records real outbound requests: URL / params / headers
node tools/kat/probe_outbound.js lite   # run only the lite platform
```

Each script prints its result as JSON to stdout; the expected values in the corresponding Rust tests are copied from there.

## Script List

| Script | Rust module referenced | Method |
|---|---|---|
| `kat_gen.js` | `src/api/native/sign.rs`, `crypto.rs` | Calls upstream `util/helper.js` directly, once per platform |
| `kat_rsa.js` | `src/api/native/crypto.rs` | Replaces `forge.random.getBytes` with a cycling `[0x01..0x10]` to make PKCS1 padding reproducible |
| `kat_device.js` | `src/api/native/device.rs` | Replaces `Math.random` with a fixed sequence, pinning `randomString`/`getGuid` |
| `kat_notsign.js` | `src/api/native/sign.rs` | Fake `useAxios` intercepts the full params actually sent by `module/song_url.js`, `module/search_lyric.js` |
| `kat_object_order.js` | `src/api/native/sign.rs` | Key order of object-typed param values: proves upstream uses insertion order, not lexicographic |
| `kat_aes.js` | `src/api/native/crypto.rs`, `mod.rs` | Pins `Math.random`/`Date.now`/`forge.random.getBytes`, records `playlistAesEncrypt` encrypt/decrypt and the full `/register/dev` request |
| `kat_login.js` | `src/api/native/mod.rs` | Pins `Math.random`/`Date.now`, records the full requests for `/v2/qrcode`, `/v2/get_userinfo_qrcode`, `/v3/get_my_info`, `/v1/get_union_vip`; for `user_detail` additionally intercepts the raw RSA plaintext |
| `kat_cloud.js` | `src/api/native/mod.rs` | Pins `Math.random` and **the whole `Date`** (`artist_audios` uses `new Date().getTime()`; stubbing only `Date.now` leaves the timestamp irreproducible), records the full requests for nine read-only endpoints |
| `kat_write.js` | `src/api/native/mod.rs` | Pins `Math.random`/`Date.now`/`forge.random.getBytes`, records the full requests for `/playlist/add`, `/playlist/del`, `/playlist/tracks/add`, `/playlist/tracks/del`; for `playlist_del` additionally intercepts the AES key and RSA plaintext. **No fake axios** — instead the baseURL is pointed at a local fake server, so the `Content-Type` axios adds in `dispatchRequest` also lands in the baseline |
| `kat_request.js` | `src/api/native/transport.rs`, `mod.rs` | Pins `Date.now`/`Math.random`, records the URL/params/headers/body built by `request.js` for byte-for-byte assertions |
| `probe_outbound.js` | `src/api/native/` (network layer) | A local fake gateway records real outbound requests for native to align item by item |

`probe_outbound.js`'s injected identity is the cookie set from the `server.js:230-266` middleware: `KUGOU_API_MID = calculateMid(guid)`, `KUGOU_API_GUID`, `KUGOU_API_DEV`, `KUGOU_API_MAC`, `KUGOU_API_WEBGL`, `dfid`. `Math.random` is likewise fixed, otherwise `song_url`'s internal `randomString(24)` differs each time and can't be compared.

`krc_probe.json` is a real `/lyric` response sample separate from `kat_notsign.js`; the decryption KAT in `krc.rs` uses its `content` field (per-character lyrics, including `[language:]` tags).

`krc_real_qt.json` is a real sample of a **whole song** (Jay Chou's 《晴天》, 68 lines, 63 with time tags), saved verbatim from the Node version's `decode=true` `/lyric` response. Its `decodeContent` is **the plaintext upstream decrypted itself**; `real_song_decode_matches_node_byte_for_byte` in `api/native/mod.rs` asserts that native's decryption is byte-for-byte identical, and also pins the six per-character intervals of the `故事的小黄花` line — this case covers both "identical decryption algorithm" and "identical per-character timestamps", and is the acceptance basis for phase 4.

Capture method (using the local Node service, `decode=true` makes it emit `decodeContent` too):

```sh
curl -s "http://127.0.0.1:3001/lyric?id=<lyric_id>&accesskey=<accesskey>&fmt=krc&decode=true&charset=utf8"
```

On check-in only the seven fields `status/error_code/fmt/contenttype/content/decodeContent/id` are kept; the rest (`info`, `_source`, `charset`) are unrelated to lyrics.

## Known Pitfalls

- **Timing of `process.env.platform`**: upstream `util/helper.js` and the modules read `platform` only at call time, but the `require` cache makes the value captured at helper top-level stick. `kat_gen.js` does `delete require.cache` for each platform, and `kat_notsign.js`'s `capture()` explicitly takes a `platform` argument — an early version omitted it once, mixing Standard params with the lite salt into a nonexistent signature (`df1eda84…`), which took a long time to track down.
- **`notSign: true` is a dead parameter**: `util/request.js:126` reads `options.notSignature`; nothing in the whole repo reads `notSign`. Don't take it literally as "skip signing".
- **`probe_outbound.js` does not sort params, and its inputs must align**: the parameter order is itself the signature input (`signatureAndroidParams` does `sort(key)` then concatenates), an early version printed in alphabetical order and buried the real order; moreover the input values (`pagesize`, `quality`, `hash`) affect the signature, and arbitrary ones produce a different request that can't be compared with native.
- **axios's query encoding is not `encodeURIComponent`**: `encode()` in `node_modules/axios/lib/helpers/buildURL.js` allows `:` `$` `,` through after `encodeURIComponent`, and writes `%20` as `+`. Written per `encodeURIComponent`, `,`, `:`, `$` get an extra encoding layer and the URL is no longer byte-for-byte identical to the Node version. Note the same-named `encode` in `AxiosURLSearchParams.js` is **a different function** (only used when an encoder is explicitly passed); don't copy that one. The baseline was captured by starting a local server that prints both `axios.getUri()` and `req.url`, which are character-for-character identical (axios 1.20.0).
- **Parameter order is the signature input**: `signatureAndroidParams` does `sort(key)` then `.map(\`${k}=${v}\`)`; `signatureWebParams` does `.map(\`${k}=${v}\`)` then `.sort()` (it sorts the rendered string, not the keys). The two orders differ; don't swap them.
- **`playlistAesEncrypt`'s key and IV are the hex characters themselves**: the first 16 and last 16 characters of `cryptoMd5(key)` are used directly as bytes (`utf8WordArray` runs `CryptoJS.enc.Utf8.parse` on the string), **not** the 8 bytes after hex decoding. Getting it wrong throws no error — the ciphertext is just all wrong and the server can't decrypt it.
- **`kat_aes.js`'s fake axios must encrypt the response with the key the module itself will generate**: the first thing the module does on entry is `randomString(6)` inside `playlistAesEncrypt(dataMap)`, with no other random consumption in between, so compute `moduleKey` from the same random sequence first, encrypt the response with it, then reset `randomIndex` before calling the module. Otherwise the module decrypts someone else's ciphertext with its own key and reports `RangeError: Invalid typed array length: -65` rather than "decryption failed".
- **Verifying AES on the node side needs `setAutoPadding(false)`**: `createCipheriv` includes PKCS#7 by default, adding a whole extra block, so the ciphertext won't match upstream and no error is raised.
- **`login_qr_key.js` has two different appids**: `params.appid` is always `1001`, while what's concatenated in `qrcode_txt` is the platform appid imported from `util/index.js` (Standard `1005`, Concept `3116`). Treating the two as one makes both `qrcode_txt` and the signature wrong.
- **`user_detail.js`'s raw RSA plaintext key order is `token` first, `clienttime` second**: the object literal order of `cryptoRSAEncrypt({ token, clienttime })` is the RSA input byte order; reversing the key order yields a different ciphertext and the server just returns an auth failure.
- **`artist_audios.js`'s time comes from `new Date()`, not `Date.now()`**: the module uses `Math.floor(new Date().getTime() / 1000)`; if `kat_cloud.js` stubs only `Date.now` it produces a real timestamp and `key` and `signature` differ each time. The whole `global.Date` must be replaced.
- **Read-only endpoints' inputs must be strings**: NodeApi goes over HTTP to the local Node service, and Express parses query values as strings. Passing numbers changes the branch of upstream's `params.x || fallback` — `withsong: 0` is falsy and becomes `1`, whereas `'0'` is truthy and kept as-is; the two signatures differ. `kat_cloud.js` therefore passes strings throughout.
- **`playlist_add.js`'s `params.type === 0` is never true**: Express gives `type` as the string `'0'`, and strict equality against the number `0` is false, so `is_pri` is not overwritten by `params.is_pri || 0`, and `params` doesn't get `last_time`/`last_area`. Written as "looks like 0 so treat as 0" both the URL and signature come out wrong.
- **`playlist_add.js`'s `list_create_userid`/`list_create_listid` get dropped entirely**: when not passed they're `undefined`, and `JSON.stringify` skips the key, leaving only 9 keys in the body. Writing `null` or `0` adds keys.
- **`kat_write.js` uses a local fake server instead of a fake axios**: `Content-Type` isn't written by `module/*.js` but added by axios in `dispatchRequest` based on the `data` type (object → `application/json`, string → `application/x-www-form-urlencoded`). A fake axios intercepts `config.headers` and misses this step; only letting the request actually reach a fake server on `127.0.0.1` captures it.
- **Standard Edition's `playlist_del` `p` and `/register/dev`'s `p` being byte-for-byte identical is not a coincidence**: both recordings' RSA plaintext is `{"aes":"15iw0r","uid":"10001","token":"TOKENFIXTURE"}` and both paddings are `0x01..0x10` (lite's public key and padding sequence differ, and the two don't match). Recorded independently yet identical, it cross-verifies that `rsaEncrypt2` (PKCS#1 v1.5) and `cryptoRSAEncrypt` (raw RSA) solve identically **for this one input set** — but the padding rules differ, so they can't be substituted for each other.
