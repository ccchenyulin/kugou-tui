# Disclaimer and Compliance Notes

Read this before using the project. For a short version see [README](../README.md#license-and-disclaimer); this page has the full terms.

1. **A personal-use tool for learning and technical research only.** It provides, hosts, and distributes no music content.

2. **The API protocols come from a third-party reverse-engineering project**: this project's embedded KuGou API implementation (signing, device fingerprint, KRC lyric decryption, QR login, cloud playlist read/write) is ported item by item against the open-source project [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi), and it also retains the `--api node` fallback path that calls that service. This project owns no music copyright and is not responsible for the availability, stability, or legality of these APIs.

3. **Third-party service compliance risk**: KuGou's (KuGou / Tencent Music Entertainment Group) related services are governed by their user agreements and local laws and regulations. Accessing them through unofficial APIs may violate their terms of service, with the risk of account restriction or termination. This project acts only as a client, using your account to access KuGou's own services (embedded backend), or accessing the API service on your own machine (`--api node`); it does not encourage or assist any form of bulk downloading, distribution, or commercial use.

4. **Copyright**: Respect music copyright and support legitimate copies. Clear cache files produced during playback within a reasonable period (default cache cap 512 MiB, recycled automatically when exceeded).

5. **Region and law**: Do not use this project in violation of local laws and regulations. All consequences arising from the use of this project are borne by the user.

6. **Privacy**: Login credentials (cookie) are stored only in your local config file (permissions `0600`) and are not sent anywhere except to KuGou's official domain (embedded backend) or a local API service you deploy yourself (`--api node`). The `token` / `userid` / `dfid` / `mid` in logs are always redacted. This project has no telemetry.
