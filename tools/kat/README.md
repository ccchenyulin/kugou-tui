# KAT 对照脚本（仅供开发）

这些脚本**不参与构建**，也不是测试夹具。用途只有一个：用上游 KuGouMusicApi 的 JS 实现，在固定输入下算出基准值，供 `src/api/native/*.rs` 的已知答案测试比对。放在这里而不是丢进 `/tmp`，是因为期望值**由运行上游源码产出**，不是手抄的常量；上游改了算法或某个签名要复查时，必须能重跑同一份基准再比对，否则只剩一堆来路不明的十六进制串。

## 依赖的上游版本

```
repo: https://github.com/MakcRe/KuGouMusicApi
version: v1.6.0
commit: a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e
date: 2026-08-14
```

`kat_rsa.js` 还依赖上游的 `node_modules/node-forge`，因此必须在装好依赖（`npm install`）的 clone 里运行。

## 用法

`KUGOU_UPSTREAM` 必填，指向 KuGouMusicApi 的 clone；未设置时脚本报错退出。每个脚本把结果以 JSON 打到 stdout，对应 Rust 测试里的期望值就抄自这里。

```sh
export KUGOU_UPSTREAM=/path/to/KuGouMusicApi
node tools/kat/kat_gen.js       # 三组签名、signKey/signParamsKey、裸 RSA、calculateMid
node tools/kat/kat_rsa.js       # PKCS1 v1.5（注入固定填充）、公钥 n/e/bitLength
node tools/kat/kat_device.js    # randomString / getGuid / calculateMid（注入固定 Math.random）
node tools/kat/kat_notsign.js   # 截获真实 song_url / search_lyric 参数，复算签名
node tools/kat/kat_object_order.js  # 对象型参数值的 JSON 键序（preserve_order 的依据）
node tools/kat/kat_aes.js       # AES-128-CBC+PKCS7 与完整 /register/dev 请求
node tools/kat/kat_login.js     # 三个扫码登录接口与两个用户信息接口的完整请求
node tools/kat/kat_cloud.js     # 九个只读接口的完整请求：云端歌单/歌手/排行榜/会员领取记录
node tools/kat/kat_write.js     # 四个云端歌单写接口的完整请求
node tools/kat/kat_request.js       # 固定时间与随机，录下 request.js 构造出的完整请求
node tools/kat/probe_outbound.js    # 录真实出站请求：URL / 参数 / 请求头
node tools/kat/probe_outbound.js lite   # 只跑 lite 平台
```

## 脚本清单

| 脚本 | 对应的 Rust 模块 | 手法 |
|---|---|---|
| `kat_gen.js` | `src/api/native/sign.rs`、`crypto.rs` | 直接调上游 `util/helper.js`，每个平台各跑一次 |
| `kat_rsa.js` | `src/api/native/crypto.rs` | 把 `forge.random.getBytes` 换成循环的 `[0x01..0x10]`，让 PKCS1 填充可复现 |
| `kat_device.js` | `src/api/native/device.rs` | 把 `Math.random` 换成固定序列，钉死 `randomString`/`getGuid` |
| `kat_notsign.js` | `src/api/native/sign.rs` | 假 `useAxios` 截获 `module/song_url.js`、`module/search_lyric.js` 真正发出的完整参数 |
| `kat_object_order.js` | `src/api/native/sign.rs` | 对象型参数值的键序：证明上游用插入序而非字典序 |
| `kat_aes.js` | `src/api/native/crypto.rs`、`mod.rs` | 钉死 `Math.random`/`Date.now`/`forge.random.getBytes`，录 `playlistAesEncrypt` 加解密与完整 `/register/dev` 请求 |
| `kat_login.js` | `src/api/native/mod.rs` | 钉死 `Math.random`/`Date.now`，录 `/v2/qrcode`、`/v2/get_userinfo_qrcode`、`/v3/get_my_info`、`/v1/get_union_vip` 的完整请求；`user_detail` 另截获 RSA 原文 |
| `kat_cloud.js` | `src/api/native/mod.rs` | 钉死 `Math.random` 与**整个 `Date`**（`artist_audios` 用 `new Date().getTime()`，只 stub `Date.now` 时间戳仍不可复现），录九个只读接口的完整请求 |
| `kat_write.js` | `src/api/native/mod.rs` | 钉死 `Math.random`/`Date.now`/`forge.random.getBytes`，录 `/playlist/add`、`/playlist/del`、`/playlist/tracks/add`、`/playlist/tracks/del`；`playlist_del` 另截获 AES key 与 RSA 原文。**不用假 axios**，而是把 baseURL 指向本地假服务器，这样 axios 在 `dispatchRequest` 里补的 `Content-Type` 也能进基准 |
| `kat_request.js` | `src/api/native/transport.rs`、`mod.rs` | 钉死 `Date.now`/`Math.random`，录 `request.js` 构造的 URL/参数/请求头/body，供逐字节断言 |
| `probe_outbound.js` | `src/api/native/`（网络层） | 本地假网关录真实出站请求，供 native 逐项对齐 |

`probe_outbound.js` 注入的身份取自 `server.js:230-266` 中间件写入的 cookie：`KUGOU_API_MID = calculateMid(guid)`、`KUGOU_API_GUID`、`KUGOU_API_DEV`、`KUGOU_API_MAC`、`KUGOU_API_WEBGL`、`dfid`。`Math.random` 同样固定，否则 `song_url` 内部 `randomString(24)` 每次都不同，无法比对。

`krc_probe.json` 是一份 `/lyric` 真实响应样本，与 `kat_notsign.js` 无关；`krc.rs` 的解密 KAT 用它的 `content` 字段（逐字歌词，含 `[language:]` 标签）。`krc_real_qt.json` 是一首**完整歌曲**的真实样本（周杰伦《晴天》，68 行，63 行带时间标签），从 Node 版 `decode=true` 的 `/lyric` 响应原样保存；其 `decodeContent` 是**上游自己解出的明文**，`api/native/mod.rs` 的 `real_song_decode_matches_node_byte_for_byte` 断言 native 的解密逐字节一致，并钉住 `故事的小黄花` 那一行的六个逐字区间——同时覆盖「解密算法一致」与「逐字时间戳一致」，是阶段 4 的验收依据。采集方式（本地 Node 服务，`decode=true` 让它一并输出 `decodeContent`）：

```sh
curl -s "http://127.0.0.1:3001/lyric?id=<lyric_id>&accesskey=<accesskey>&fmt=krc&decode=true&charset=utf8"
```

入库时只保留 `status/error_code/fmt/contenttype/content/decodeContent/id` 七个字段，其余（`info`、`_source`、`charset`）与歌词无关。

## 已知坑

- **`process.env.platform` 的生效时机**：上游 `util/helper.js` 与各模块都在调用时才读 `platform`，但 `require` 缓存会让 helper 顶层捕获的值固化。`kat_gen.js` 对每个平台 `delete require.cache`，`kat_notsign.js` 的 `capture()` 显式接收 `platform` 参数——早期版本漏传过一次，把标准版参数和 lite 的盐混成了不存在的签名（`df1eda84…`），查了很久。
- **`notSign: true` 是死参数**：`util/request.js:126` 读的是 `options.notSignature`，全仓库没有任何地方读 `notSign`。别照着字面理解成「跳过签名」。
- **`probe_outbound.js` 不排序参数，且输入必须对齐**：参数顺序本身就是签名输入（`signatureAndroidParams` 先 `sort(key)` 再拼接），早期版本按字母序打印，把真实顺序埋掉了；另外输入值（`pagesize`、`quality`、`hash`）会影响签名，随手填会得到另一个请求，无法与 native 比对。
- **axios 的 query 编码不是 `encodeURIComponent`，且参数顺序就是签名输入**：`node_modules/axios/lib/helpers/buildURL.js` 的 `encode()` 在 `encodeURIComponent` 之后放行 `:` `$` `,`，并把 `%20` 写成 `+`；按 `encodeURIComponent` 写，`,`、`:`、`$` 会多一层编码，URL 不再与 Node 版逐字节一致。注意 `AxiosURLSearchParams.js` 里同名的 `encode` 是**另一个函数**（只在显式传 encoder 时用），别抄那个。基准是用本地服务器同时打印 `axios.getUri()` 与 `req.url` 采到的，两者逐字符相同（axios 1.20.0）。顺序方面：`signatureAndroidParams` 先 `sort(key)` 再 `.map(\`${k}=${v}\`)`；`signatureWebParams` 先 `.map(\`${k}=${v}\`)` 再 `.sort()`（排的是渲染后的字符串，不是键）——两者不同，别互换。
- **`playlistAesEncrypt` 的 key 与 IV 就是十六进制字符本身**：取 `cryptoMd5(key)` 的前 16 和后 16 个**字符**直接当字节用（`utf8WordArray` 对该字符串跑 `CryptoJS.enc.Utf8.parse`），**不是**十六进制解码后的 8 字节。弄错不会报错——密文全错，服务端解不开而已。在 node 侧验证 AES 要 `setAutoPadding(false)`：`createCipheriv` 默认带 PKCS#7，会多补一整块，密文对不上上游且不报错。**`kat_aes.js` 的假 axios 必须用模块自己将要生成的 key 加密响应**：模块进来第一件事是 `playlistAesEncrypt(dataMap)` 里的 `randomString(6)`，中间没有别的随机消费，所以先用同一条随机序列算出 `moduleKey`、用它加密响应，再把 `randomIndex` 归零后才调模块；否则模块拿自己的 key 解别人的密文，报的是 `RangeError: Invalid typed array length: -65`，而不是「解密失败」。
- **`login_qr_key.js` 有两个不同的 appid**：`params.appid` 恒为 `1001`，而拼进 `qrcode_txt` 的是从 `util/index.js` 导入的平台 appid（标准版 `1005`，概念版 `3116`）。把两者当成一个，`qrcode_txt` 和签名都会错。
- **`user_detail.js` 的 RSA 原文键序是 `token` 在前、`clienttime` 在后**：`cryptoRSAEncrypt({ token, clienttime })` 的对象字面量顺序就是 RSA 输入字节序；键序反过来会得到不同密文，服务端只会返回鉴权失败。
- **`artist_audios.js` 的时间来自 `new Date()` 而非 `Date.now()`**：模块用的是 `Math.floor(new Date().getTime() / 1000)`；`kat_cloud.js` 只 stub `Date.now` 的话会产出真实时间戳，`key` 与 `signature` 每次都不同。必须替换整个 `global.Date`。
- **只读接口的入参必须是字符串**：NodeApi 走 HTTP 打到本机 Node 服务，Express 把 query 值一律解析成字符串。传数字会改变上游 `params.x || fallback` 的分支——`withsong: 0` 是 falsy 会被换成 `1`，而 `'0'` 是真值原样保留，两者签名不同。因此 `kat_cloud.js` 全程传字符串。
- **`playlist_add.js` 的 `params.type === 0` 永远不成立，`list_create_userid`/`list_create_listid` 会被整个丢掉**：Express 给的是字符串 `'0'`，与数字 `0` 严格相等为假，于是 `is_pri` 不会被 `params.is_pri || 0` 覆盖，`params` 也拿不到 `last_time`/`last_area`；按「看起来是 0 就当 0」写，URL 和签名都会错。两个 `list_create_*` 不传时是 `undefined`，`JSON.stringify` 跳过该键，body 只剩 9 个键——写成 `null` 或 `0` 反而多出键。
- **`kat_write.js` 用本地假服务器而非假 axios**：`Content-Type` 不是 `module/*.js` 写的，而是 axios 在 `dispatchRequest` 里按 `data` 类型补的（对象 → `application/json`，字符串 → `application/x-www-form-urlencoded`）。假 axios 拦的是 `config.headers`，看不到这一步；只有让请求真发到 `127.0.0.1` 上的假服务器才能录到。
- **标准版 `playlist_del` 的 `p` 与 `/register/dev` 的 `p` 逐字节相同不是巧合**：两份记录的 RSA 原文都是 `{"aes":"15iw0r","uid":"10001","token":"TOKENFIXTURE"}`，填充都是 `0x01..0x10`（lite 的公钥与填充序列不同，两者对不上）。各自独立录制却相同，交叉验证了 `rsaEncrypt2`（PKCS#1 v1.5）与 `cryptoRSAEncrypt`（裸 RSA）**对这一组输入**解出相同结果——但填充规则不同，两者不能互相替代。
