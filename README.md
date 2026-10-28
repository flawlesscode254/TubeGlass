<div align="center">

# TubeGlass

**A desktop client for YouTube, YouTube Music and podcasts — no account, no API key, no ads.**

[![CI](https://github.com/flawlesscode254/TubeGlass/actions/workflows/ci.yml/badge.svg)](https://github.com/flawlesscode254/TubeGlass/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/flawlesscode254/TubeGlass?include_prereleases&label=release)](https://github.com/flawlesscode254/TubeGlass/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows%20%7C%20macOS-informational)

<img src="docs/screenshots/home.png" alt="TubeGlass home page" width="860">

</div>

TubeGlass is what [NewPipe](https://github.com/TeamNewPipe/NewPipe) is on Android, for the desktop:
follow the channels you like, watch and listen without an account, save things for offline use, and
keep everything on your own computer. It is written in Rust with [egui](https://github.com/emilk/egui),
decodes with [ffmpeg](https://ffmpeg.org) (included — nothing else to install) and gets its data the way NewPipe does — by reading
YouTube's public pages and internal endpoints, so there is no API key to register and no quota.

> TubeGlass is an independent project. It is not affiliated with or endorsed by YouTube or Google.
> Use it for your own viewing and respect the terms of the services you access.

## Contents

- [Screenshots](#screenshots)
- [Features](#features)
- [Install](#install) — and [do I need ffmpeg?](#do-i-need-to-install-ffmpeg)
- [Updating](#updating)
- [A short tour](#a-short-tour)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Where your data lives](#where-your-data-lives)
- [Building from source](#building-from-source)
- [Troubleshooting](#troubleshooting)
- [How it works](#how-it-works)
- [Contributing](#contributing) · [Releasing](#releasing) · [License](#license)

## Screenshots

| | |
| :---: | :---: |
| <img src="docs/screenshots/player.png" width="420"><br>**Player** — qualities, speed, chapters, captions, ambient glow | <img src="docs/screenshots/music-player.png" width="420"><br>**Music** — synced lyrics under the audio visualizer |
| <img src="docs/screenshots/music-search.png" width="420"><br>**YouTube Music search** — songs, albums, artists, playlists | <img src="docs/screenshots/podcasts.png" width="420"><br>**Podcasts** — subscribe to any RSS feed |
| <img src="docs/screenshots/podcast-player.png" width="420"><br>**Podcast player** — its own visualizer, transcripts as captions | <img src="docs/screenshots/clip-tool.png" width="420"><br>**Clips** — cut a piece out as MP4, M4A or GIF |
| <img src="docs/screenshots/downloads.png" width="420"><br>**Downloads** — offline library with grid, compact and list views | <img src="docs/screenshots/clips.png" width="420"><br>**Clip library** — every clip, with a link back to its source |
| <img src="docs/screenshots/home-light.png" width="420"><br>**Light theme** — dark, light or follow the system | <img src="docs/screenshots/settings.png" width="420"><br>**Settings** — equalizer, gapless, crossfade and more |

## Features

### Watching
- **Channels** — add one by `@handle`, link or id. Each channel page has Videos, Live, Shorts,
  Playlists and About tabs with infinite scroll and a search box that searches inside the channel.
  A channel can be unsubscribed from right on its page, and its videos (like any playlist) can be
  sorted by date, views, length or title and narrowed by date range and length.
- **A feed that is already there** — everything is cached on disk, so the app opens with content;
  *Refresh* fetches what is new. New videos get a **NEW** badge. Live streams and upcoming
  premieres are marked.
- **Live status** — a live stream shows a red *LIVE* badge while you are in step with the broadcast; pause or buffer and it turns into *“12s behind”* with a **Go live** button that jumps back to the live edge.
- **Groups** — put channels into named groups (Music, News …), each with its own feed.
- **Search** — videos, channels and playlists, with suggestions, recent searches and sort / date /
  length filters. Paste a video or playlist link and it opens directly.
- **History with resume** — every video remembers where you stopped; *Watch later* and your own
  playlists are kept locally. **Import playlists from YouTube** by pasting their links (the download
  arrow next to *Playlists* in the sidebar); public and unlisted playlists work, private ones do not.
- **Subscriptions import / export** — NewPipe JSON, YouTube Takeout CSV, OPML or a plain list of links.
- **"For you" ordering** *(optional)* — Home can rank videos by what you actually watch; right-click
  a video for *Not interested* or *Recommend less from this channel*.

### The player
- Quality selection, speed 0.5×–2× (remembered per channel), fullscreen, **picture-in-picture** and a
  compact **mini player**; navigating away keeps playback going in a floating window or a bottom bar.
- **Chapters** from the description, with a thumbnail preview of any point on the seek bar.
- **Captions** with automatic **translation** into 13 languages, a searchable **transcript** panel
  and timestamped **notes** (shown as dots on the seek bar).
- **SponsorBlock** segment skipping (off by default), **sleep timer**, repeat and shuffle.
- A **queue** you can reorder by dragging and save as a playlist; **gapless playback** with an
  optional **crossfade** between songs.
- **Ambient mode** — a soft glow of the picture's colours behind the player.
- Audio-only mode with a live **visualizer**; an **equalizer**, volume levelling and silence skipping
  for spoken audio.
- The controls fade out like YouTube's and stay while you use a menu on top of them.

### Music
YouTube Music search (songs, albums, artists, playlists), album and artist pages, **synced lyrics**
(LRCLIB with a YouTube Music fallback, matched against the song so unrelated lyrics are not shown),
liked songs, followed artists and personal recommendations built from what you play — all without
an account.

### Podcasts
Search the podcast directory or paste a feed address or an Apple Podcasts link and **subscribe**.
Episodes stream with resume, download for offline listening and show **NEW** markers. When a feed
publishes a transcript it appears as captions (and as a full searchable panel). Podcasts get their
own round visualizer.

### Offline
- **Resumable downloads** (pause, resume, cancel, up to two at a time, automatic retry with fresh
  links) as video at the quality you choose or as audio only.
- **Albums and playlists** download as one group; **Keep offline** keeps a playlist saved and
  fetches whatever is added later.
- **Captions, lyrics and transcripts are saved with the download**, so they also work with no
  connection. When you are offline the app notices and opens your downloads.
- A **storage manager** shows what is used and frees space from finished or old downloads.

### Clips, local files and desktop
- **Clips** — cut a range of any video to MP4, M4A or GIF, with a progress bar and *Cancel*. When the video has chapters, the Clip tab lists them: click one to select it, or press its *Clip* button to save it right away. A clip
  shows up on that video's seek bar, and the Clips page links back to the source.
- **Local media** — open any audio or video file (`Ctrl+O`, drag and drop, or *Open with*) with the
  same player and visualizer.
- **Insights** — local statistics: time watched and listened, top channels and artists.
- **Linux desktop** — media keys and the "now playing" widget through MPRIS, notifications when a
  followed channel goes live.

### Look and feel
Grid, compact grid and list views on every page, dark / light / system theme, a custom accent
colour, text size and density options, and a layout that adapts when you shrink the window.

## Install

Download the file for your system from the **[latest release](https://github.com/flawlesscode254/TubeGlass/releases/latest)**:

| System | File | How to install |
| --- | --- | --- |
| Any Linux | `TubeGlass-<version>-linux-<arch>.AppImage` | `chmod +x TubeGlass-*.AppImage && ./TubeGlass-*.AppImage` — it can update itself |
| Debian, Ubuntu, Mint … | `tubeglass_<version>_<amd64\|arm64>.deb` | `sudo apt install ./tubeglass_*.deb` (pulls in ffmpeg) |
| Other Linux | `TubeGlass-<version>-linux-<arch>.tar.gz` | unpack, then run `./install.sh` (installs for your user, no root) |
| Arch Linux | `packaging/PKGBUILD` | `cd packaging && makepkg -si` |
| Windows 10 / 11 | `TubeGlass-<version>-windows-x86_64.zip` | unzip anywhere and run `TubeGlass.exe` |
| macOS 11 or newer | `TubeGlass-<version>-macos-<aarch64\|x86_64>.dmg` | open it and drag TubeGlass to *Applications* |

`<arch>` is `x86_64` (Intel/AMD) or `aarch64` (ARM, e.g. Apple silicon, Raspberry Pi). Every release
also has a `SHA256SUMS.txt` so you can check your download.

**First launch on macOS and Windows.** The apps are not code-signed. On macOS, right-click
TubeGlass and choose *Open* the first time (or run `xattr -dr com.apple.quarantine /Applications/TubeGlass.app`).
On Windows, if SmartScreen warns you, choose *More info → Run anyway*.

### Do I need to install ffmpeg?

**No.** TubeGlass uses [ffmpeg](https://ffmpeg.org) for all decoding, downloading and clipping, and it
takes care of having one:

| How you got TubeGlass | Where ffmpeg comes from |
| --- | --- |
| AppImage, Linux tarball, Windows zip, macOS app | **Included in the package** — it works offline straight after installing |
| `.deb` | Installed by `apt` as a dependency of the package |
| Built from source (`cargo run`), or any other install without ffmpeg | **Downloaded automatically the first time the app starts** (about 30 MB, once, with a progress bar at the top of the window). It is checked against a pinned SHA-256 checksum and stored in TubeGlass's own data folder, so nothing is installed system-wide |

TubeGlass looks for ffmpeg in this order: the `TUBEGLASS_FFMPEG` environment variable → the copy
that came with the package (next to the program) → the copy it downloaded earlier → `ffmpeg` on your
`PATH`. **Settings → Data → ffmpeg** shows which one is in use and can download a fresh copy.

If the automatic download cannot work (no internet on first run, or a system with no ready-made
build), a red bar explains and offers *Try again*; it retries by itself when you are back online.
You can always install ffmpeg yourself instead (`sudo apt install ffmpeg`, `brew install ffmpeg`,
`winget install ffmpeg`).

The bundled builds are static **GPL v3** builds of FFmpeg, run as a separate program (TubeGlass itself
stays MIT). Their licence, where they come from and how to get their source are in
[`packaging/THIRD_PARTY_NOTICES.md`](packaging/THIRD_PARTY_NOTICES.md), and the exact files and
checksums are pinned in [`packaging/ffmpeg-sources.tsv`](packaging/ffmpeg-sources.tsv).

### Platform status

| | Linux | macOS | Windows |
| --- | :---: | :---: | :---: |
| Builds and passes the tests in CI | ✅ | ✅ | ✅ |
| Packaged by the release workflow | ✅ x86_64, aarch64 | ✅ Apple silicon, Intel | ✅ x86_64 |
| Playback, downloads, podcasts, clips | ✅ | ✅ | ✅ |
| ffmpeg included | ✅ (not in the `.deb`: `apt` installs it) | ✅ | ✅ |
| Media keys / "now playing" widget | ✅ MPRIS | — | — |
| Desktop notifications | ✅ | ✅ | — |
| Double-click / "Open with" a file | ✅ | — *(use Open file…)* | — *(use Open file…)* |
| Pause an ffmpeg (HLS) download | ✅ | ✅ | — *(keeps running)* |

Linux is the most-tested platform. The macOS and Windows builds are compiled, tested and packaged
automatically on every change, but have had less hands-on use — please report anything odd.

## Updating

TubeGlass looks at this repository's releases a few seconds after it starts, and again every few
hours while it stays open (at most once every six hours across launches). Nothing is sent except
the request for the latest release. When a newer version exists a bar appears:

- **Update** — downloads the file that fits your installation (AppImage, `.deb`, tarball, Windows zip
  or macOS dmg), checks its size, then installs it where it can. An **AppImage replaces itself** and
  offers *Restart now*; the other formats open in your package installer or file manager. If there
  is no file for your system, the release page opens.
- **What's new** — the release notes.
- **Skip this version** — no more reminders until a newer one appears.

You can switch the automatic check off, or check right now, in **Settings → Data**.

## A short tour

1. **Add something to watch.** Type `@NASA` (or a channel link) in the box at the top of the sidebar
   and press *Add*. Its latest videos and live streams appear on **Home**.
2. **Play.** Click a card. Hover over the seek bar for a preview; press `Space` to pause; click the
   picture to pause too. Leave the page and playback continues in a small window.
3. **Save for later.** Right-click a card for *Watch later*, *Add to playlist*, *Download* or
   *Play audio only*. Downloads live under **Downloads**; albums and playlists group together.
4. **Listen.** Open **Music** to search songs, albums and artists, or **Podcasts** to paste a feed.
5. **Make a clip.** While playing, open the **Clip** tab (or `…` → *Clip this moment*), choose a
   range and a format, and press *Save clip*. Find it on the **Clips** page.
6. **Make it yours.** **Settings** has quality, audio, captions, theme, accent colour, the download
   folder, backup and restore, and the update check.

Tips: right-click almost anything for more actions; `?` shows every shortcut; the three small icons
above any list switch between grid, compact grid and list.

## Keyboard shortcuts

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `Space` / `K` | Play or pause | `A` | Audio only on / off |
| `←` `→` | Seek 5 seconds | `I` | Mini player on / off |
| `J` / `L` | Seek 10 seconds | `B` | Ambient mode on / off |
| `↑` `↓` | Volume | `W` | Watch later on / off |
| `M` | Mute | `C` | Comments (transcript for podcasts) |
| `[` / `]` | Slower / faster | `Y` | Lyrics (music) |
| `N` / `P` | Next / previous in queue | `F` | Fullscreen |
| `Esc` | Back (keeps playing in PiP) | `Ctrl+O` | Open a file |
| `Ctrl+B` | Collapse the sidebar | `?` | Show this list |

## Where your data lives

Everything stays on your computer. Settings and lists are small JSON files you can back up (or use
**Settings → Back up everything**).

| | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Settings and lists | `~/.config/tubeglass/` | `~/Library/Application Support/tubeglass/` | `%APPDATA%\tubeglass\` |
| Cache (thumbnails, feeds) | `~/.cache/tubeglass/` | `~/Library/Caches/tubeglass/` | `%LOCALAPPDATA%\tubeglass\` |
| Downloads and clips | `~/Videos/TubeGlass/` | `~/Movies/TubeGlass/` | `Videos\TubeGlass\` |

The download folder can be changed in **Settings → Downloads**. The settings folder holds
`channels.json`, `settings.json`, `history.json`, `playlists.json`, `groups.json`, `podcasts.json`,
`clips.json`, `bookmarks.json`, `stats.json` and a few more.

**What leaves your computer:** requests to YouTube / YouTube Music for what you open, to
[SponsorBlock](https://sponsor.ajay.app) (hashed prefixes only, if enabled), to
[LRCLIB](https://lrclib.net) for lyrics, to the podcast directory and feeds you subscribe to, and to
GitHub for the update check. No analytics, no accounts, no telemetry.

## Building from source

### 1. Prerequisites

You need a [Rust toolchain](https://rustup.rs) (stable, 2024 edition — Rust 1.87 or newer) and a
few development libraries. **You do not need to install ffmpeg**: the first time the app starts it
downloads its own (see [above](#do-i-need-to-install-ffmpeg)). Installing ffmpeg yourself is optional
and is used instead when it is on your `PATH`.

<details open>
<summary><b>Linux</b></summary>

```bash
# Debian / Ubuntu
sudo apt install build-essential pkg-config libasound2-dev libxkbcommon-dev libwayland-dev libx11-dev

# Fedora
sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel libxkbcommon-devel wayland-devel libX11-devel

# Arch
sudo pacman -S --needed base-devel alsa-lib libxkbcommon wayland libx11
```
</details>

<details>
<summary><b>macOS</b></summary>

```bash
xcode-select --install          # compiler and SDK
```
</details>

<details>
<summary><b>Windows</b></summary>

Install the [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) with the
*Desktop development with C++* workload. That is all.
</details>

### 2. Get the code and run it

```bash
git clone https://github.com/flawlesscode254/TubeGlass.git
cd TubeGlass
cargo run --release            # build and start (the first build takes a few minutes;
                               # on first start the app fetches its own ffmpeg)
```

`--release` matters: debug builds decode video far too slowly to play smoothly.

To install it for your user on Linux, with a menu entry and "Open with" support:

```bash
./install.sh                   # build and install to ~/.local
./install.sh --default         # ...and make it the default player for audio and video
./install.sh --uninstall
```

### 3. Test

```bash
cargo test --release                    # fast unit tests (parsers, update logic, backups …)
cargo test --release -- --ignored       # also the live tests; need internet access (they download ffmpeg too)
```

The live tests talk to YouTube, so they are marked `#[ignore]` and run daily in the *YouTube canary*
workflow — a failure there means YouTube changed something and the extractor needs a fix.

### 4. Build the release packages

All scripts take their version from `Cargo.toml` and write to `dist/`. Set `SKIP_BUILD=1` to package
a binary you already built, and `SKIP_FFMPEG=1` to leave the bundled ffmpeg out (the downloads are
checked against `packaging/ffmpeg-sources.tsv`; `packaging/fetch-ffmpeg.sh <platform> <folder>` fetches one by hand).

| Package | Run it on | Command |
| --- | --- | --- |
| `.deb` | Linux | `./packaging/build-deb.sh` |
| AppImage | Linux (needs [appimagetool](https://github.com/AppImage/appimagetool)) | `./packaging/build-appimage.sh` |
| `.tar.gz` | Linux | `./packaging/build-tarball.sh` |
| `.dmg` | macOS | `./packaging/build-macos.sh [aarch64\|x86_64]` |
| `.zip` | Windows | `cargo build --release --locked; ./packaging/build-windows.ps1` |

### Environment variables

| Variable | Effect |
| --- | --- |
| `TUBEGLASS_FFMPEG=<path>` | Use this ffmpeg instead of the bundled, downloaded or system one |
| `TG_DEBUG=1` | Print player timings, state changes and gapless hand-overs to stderr |
| `TUBEGLASS_UPDATE_API=<url>` | Point the update checker at another "latest release" JSON (for testing the update flow) |

### Project layout

```
src/
  main.rs        entry point and the crate-wide imports
  app/           application state and logic: playing, queue, downloads, navigation,
                 background messages, desktop integration, the updater
  ui/            everything that draws: pages, widgets, theme, icons, podcast and music screens
  data/          settings, files on disk, backup, and the types shared across the app
  youtube/       YouTube and YouTube Music extraction (innertube clients, scraping, lyrics)
  playback/      the ffmpeg + cpal player, the local range proxy, local files, MPRIS
  library/       downloads, clips, podcasts, subscriptions import, statistics, update checker
  platform.rs    the few things that differ between Linux, macOS and Windows
  util.rs        small helpers
build.rs         embeds the icon in the Windows executable
packaging/       .deb, AppImage, tarball, macOS .app / .dmg, Windows zip, PKGBUILD, icons
docs/            screenshots
.github/         CI, release and daily YouTube-canary workflows
```

Every source file starts with a `//!` description and every function has a doc comment, so
`cargo doc --open` gives a browsable map of the code.

## Troubleshooting

| Problem | What to do |
| --- | --- |
| A red bar says **ffmpeg is not available** | The automatic download failed (offline, or a blocked connection). Press *Try again*, or install ffmpeg yourself (`apt` / `brew` / `winget`) and restart TubeGlass. |
| **"Can't play this video"** | TubeGlass retries once with other YouTube clients by itself; press *Try again*. If a video keeps failing, run **Settings → Check YouTube access** to see which part broke. YouTube changes things now and then — check for an update. |
| No sound | Check the system's default output device; TubeGlass plays through it. On Linux the user must be able to open the ALSA / PipeWire device. |
| Video stutters | Use a release build (`cargo run --release`), or lower the quality in the player. |
| AppImage does not start | It needs FUSE; install `libfuse2` (Ubuntu 22.04+: `sudo apt install libfuse2`) or run it with `--appimage-extract-and-run`. |
| Blank or wrongly scaled window on Wayland | Start it under X11 for a test: `env -u WAYLAND_DISPLAY ./tubeglass`, and please open an issue. |
| macOS says the app is damaged / from an unidentified developer | Right-click → *Open*, or `xattr -dr com.apple.quarantine /Applications/TubeGlass.app`. |
| Podcast feed "is not a podcast feed" | Paste the RSS address (ending in `.xml`, `/feed`, …), not the show's web page — or just search for the show's name. |

## How it works

- **Extraction.** Streams come from YouTube's internal player endpoint, asked as several official
  clients in turn (the primary one is verified to hand out unciphered links, the others are fallbacks
  that are tested before use). Channel, search and comment data are read from the same JSON the web
  pages use. Everything has a plain-text error path so a failure says what went wrong.
- **Playback.** YouTube only serves bounded ranges, so the bundled ffmpeg reads through a tiny loopback
  **range proxy** that fetches the file in chunks. ffmpeg writes raw RGBA pictures and PCM sound; the
  sound goes to the sound card with [cpal](https://github.com/RustAudio/cpal) and its sample counter
  is the **master clock** that pictures are paced against, so sound and picture stay together through
  seeks and speed changes. Gapless playback pre-decodes the next track and splices it in.
- **Offline.** Downloads fetch the raw streams in resumable chunks and mux them with ffmpeg; a small
  JSON sidecar per file makes the library listable and playable with no network.
- **Interface.** An immediate-mode [egui](https://github.com/emilk/egui) UI. Background threads
  (network, ffmpeg) send results to the UI thread as messages.
- **Portability.** `platform.rs` holds the OS-specific parts (opening files, ending processes, free
  disk space, console-less helper processes); on Windows the sound is decoded by its own ffmpeg
  process instead of a passed file descriptor.

## Contributing

Issues and pull requests are welcome. Before opening one:

```bash
cargo build --release && cargo test --release
```

CI runs the same on Linux, Windows and macOS. Please keep commits focused with a message that says
*what* changed and *why*. If you find a video that will not play, include its link and the output of
**Settings → Check YouTube access**.

## Releasing

1. Raise `version` in `Cargo.toml` and commit it.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The *Release* workflow builds every package (Linux x86_64 + aarch64, Windows, macOS Apple silicon +
   Intel), checks that the tag matches `Cargo.toml`, and publishes the release with checksums and
   generated notes. Running the workflow by hand, or pushing the `release-dryrun` branch, builds
   everything without publishing.

The in-app update checker picks its download by file name, so keep the naming scheme in
`packaging/` and `.github/workflows/release.yml` as it is.

## License

[MIT](LICENSE) © 2026 flawlesscode254
