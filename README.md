# Origami

A video editor where the edit is a short JavaScript file. Write the script, watch the preview, and drag clips, keys and sliders in the app: every click rewrites the script, so the file is always the whole project.

```js
settings({ fps: 30, size: [1920, 1080] })
cat(
  video("clips/intro.mp4").trim(0, 4),
  stack(
    video("clips/funny.mp4").trim(2, 8),
    caption("we made it").at(1).for(2),
  ),
)
```

Origami runs on Macs with Apple silicon (M1 or later) and macOS 15 or later. The app includes the ffmpeg it uses to read and export video.

## Install

### Download

1. Download [Origami.dmg](https://origami.video/Origami.dmg) from [origami.video](https://origami.video), open it and drag Origami to Applications. It is for Apple silicon only (M1 or later), macOS 15 or later.
2. Open Origami from Applications. It is signed with an Apple Developer ID and notarized, so macOS asks once whether to open an app downloaded from the internet: click **Open**.

### Build from source

You need the Xcode Command Line Tools (`xcode-select --install`), Rust from [rustup.rs](https://rustup.rs) and ffmpeg (`brew install ffmpeg`): only the downloaded app includes ffmpeg, so a build from source and the tests use the one on your PATH.

```
git clone https://github.com/DannyMang/origami.git
cd origami
cargo run --release --manifest-path origami-app/Cargo.toml -- path/to/edit.js
```

With no script it opens the launch screen. To make an app you can keep in Applications, run `./bundle.sh`: it builds `dist/Origami.app` and `dist/Origami.dmg`.

## Your first project

1. Open Origami and click New project on the launch screen, or press ⌘N. Type a name for the project, pick where it goes in the folder list (the first time, it starts in `Movies/Origami`), and press Enter or click Create. Origami makes a folder with that name holding an `edit.js` script and opens it. It shows "hello origami".
2. Put a video at least 8 seconds long in that folder, at `clips/intro.mp4`, and replace the script with:

   ```js
   cat(
     video("clips/intro.mp4").trim(0, 3),
     stack(
       video("clips/intro.mp4").trim(5, 8),
       text("hello").at(1).for(2),
     ),
   )
   ```

3. Press ⌘S. The timeline now holds three seconds of the video, then three more with "hello" over the last two.
4. Drag the end of a clip on the timeline: the numbers in `.trim(…)` change with it.
5. Press ⇧⌘E to export an MP4.

To open a script later, use ⌘O, or Control-click it in Finder and choose Open With › Origami.

The app also contains the `origami` command-line tool, which renders a script without opening a window:

```
/Applications/Origami.app/Contents/MacOS/origami render edit.js out.mp4
```

## Docs

The guide and language reference are in [`docs/pages`](docs/pages): [Intro](docs/pages/00-intro.md), [Basics](docs/pages/01-basics.md), [Video](docs/pages/02-video.md), [Arranging](docs/pages/03-arranging.md), [Methods](docs/pages/04-methods.md), [Color](docs/pages/05-color.md), [Editor](docs/pages/06-editor.md), [Motion](docs/pages/07-motion.md), [Supported formats](docs/pages/07-supported-formats.md) and the [Changelog](docs/pages/08-changelog.md).

Read them online, with every example rendered, at https://origami.video.

`CONTRIBUTING.md` maps the code and `docs/README.md` explains how to build the docs.

## What stays on your Mac

Your scripts, footage and exports stay on your Mac. Origami keeps:

- `origami.lock` next to each script, which records the media the script uses.
- Saved versions of each script, unsaved code kept after a crash, your export choices, the Open Recent list, the folder you last made a project in and your answer about reports, in `~/Library/Application Support/Origami`.
- A crash log in `~/Library/Logs/Origami`.
- What ffmpeg said about your footage (frame rates, timings and where each clip's first frame lands), in `~/Library/Caches/Origami/media.json`, so the next launch does not ask again. It lists the paths of the media files you have used. It is safe to delete, and macOS may clear it.

What it sends, in short:

- **Update check.** Each time it opens, Origami downloads one small file from origami.video to see if a newer version is out. It sends nothing about you or your project; GitHub sees the request come from your internet address, as with any download. Start it with `ORIGAMI_NO_UPDATE_CHECK=1` to turn this off.
- **Usage and crash reports, only if you say yes.** The launch screen asks once. Nothing is sent before you click Yes or after you click No thanks. After a yes, a few anonymous events go to PostHog, such as "app opened" and "exported", with a random install id and no script contents, file names, paths or user name. Each carries the Origami and macOS versions and the kind of chip; "exported" adds the format, size, length and whether it worked, and a crash report only the kind of error and the file and line in Origami's own code where it happened. Origami › Share Usage and Crash Reports turns them off, and `ORIGAMI_NO_TELEMETRY=1` sends nothing and never asks.
- `ORIGAMI_NO_TELEMETRY` and `ORIGAMI_NO_UPDATE_CHECK` only apply when Origami is started from a terminal that sets them, such as `ORIGAMI_NO_TELEMETRY=1 /Applications/Origami.app/Contents/MacOS/origami-app`; opening it from the Dock or Finder does not read them.
- **Built from source:** a copy you build yourself never asks and never sends reports, unless you add your own PostHog key. It still checks for updates unless you turn that off.
- **Microphone:** used only while you record a voiceover.
- **Scripts** can read any file your account can read when they name it, but cannot use the network, run programs or write files other than their own `origami.lock`. Open scripts from people you trust.

## Known limits

- Apple silicon and macOS 15 or later only.
- A project is a script file (`.js`). There is no import from other editors.
- The included ffmpeg has no software AV1 decoder, so AV1 video is refused with a message on its script line: convert it to H.264 or HEVC first. SVG pictures are not read: export them as PNG. [Supported formats](docs/pages/07-supported-formats.md) lists what works.

## Report a bug

Open an issue at [github.com/DannyMang/origami/issues](https://github.com/DannyMang/origami/issues) with your macOS version, the Origami version shown on the launch screen, what you did and what happened, and the smallest script that shows it. If Origami crashed, attach `~/Library/Logs/Origami/origami.log` after checking it for anything private; if it quit without writing there, attach the newest `origami-app` report in `~/Library/Logs/DiagnosticReports` instead, checked the same way.

Report security problems privately, as [SECURITY.md](SECURITY.md) describes, not in an issue.

## Uninstall

Quit Origami and drag it from Applications to the Trash. To remove what it keeps, also delete `~/Library/Application Support/Origami` (saved versions, recovered code, export choices, Open Recent and your answer about reports) , `~/Library/Logs/Origami` and `~/Library/Caches/Origami`. Your scripts, the `origami.lock` beside each one, your footage and your exports stay where they are.

## Licence

[GPL-3.0](LICENSE), copyright 2026 Daniel Ung: use, change and share Origami for any purpose. If you share it or a changed version, share the source under the same licence. Videos you make with Origami are yours and the licence does not cover them. The open-source crates Origami is built with are listed in [THIRD-PARTY.md](THIRD-PARTY.md), with their licence texts in [THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt).
