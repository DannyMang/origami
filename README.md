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

Origami runs on Macs with Apple silicon (M1 or later) and macOS 15 or later. It uses ffmpeg, which you install yourself.

## Install

### Download

1. Install ffmpeg with [Homebrew](https://brew.sh): `brew install ffmpeg`.
2. Download `Origami.dmg` from [Releases](https://github.com/DannyMang/origami/releases/latest), open it and drag Origami to Applications.
3. Open Origami from Applications. Origami is not signed with an Apple Developer ID yet, so macOS stops it with "“Origami” Not Opened: Apple could not verify “Origami” is free of malware that may harm your Mac or compromise your privacy." Click **Done**, not Move to Trash.
4. Open **System Settings › Privacy & Security** and scroll down to Security, where it says Origami was blocked. Click **Open Anyway**. The button is there for about an hour after step 3.
5. Confirm with **Open Anyway** and your login password or Touch ID. Origami opens, and from then on it opens like any other app.

To check the download, run `shasum -a 256 ~/Downloads/Origami.dmg` in Terminal and compare the result with the SHA-256 in the release notes.

Control-click › Open no longer skips this check on macOS 15 and later. If you prefer the Terminal, run this instead of steps 3 to 5:

```
/usr/bin/xattr -dr com.apple.quarantine /Applications/Origami.app
```

Each new version you download asks again once.

### Build from source

You need the Xcode Command Line Tools (`xcode-select --install`), Rust from [rustup.rs](https://rustup.rs) and ffmpeg (`brew install ffmpeg`).

```
git clone https://github.com/DannyMang/origami.git
cd origami
cargo run --release --manifest-path origami-app/Cargo.toml -- path/to/edit.js
```

With no script it opens the launch screen. To make an app you can keep in Applications, run `./bundle.sh`: it builds `dist/Origami.app` and `dist/Origami.dmg`. An app you build yourself opens without the steps above.

## Your first project

1. Open Origami and press ⌘N. Save the new script as `edit.js` in an empty folder. It shows "hello origami".
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

If macOS stops it the first time, allow it in Privacy & Security as above, or run the `xattr` command.

## Docs

The guide and language reference are in [`docs/pages`](docs/pages): [Intro](docs/pages/00-intro.md), [Basics](docs/pages/01-basics.md), [Video](docs/pages/02-video.md), [Arranging](docs/pages/03-arranging.md), [Methods](docs/pages/04-methods.md), [Color](docs/pages/05-color.md), [Editor](docs/pages/06-editor.md), [Motion](docs/pages/07-motion.md) and the [Changelog](docs/pages/08-changelog.md).

Read them online, with every example rendered, at DOCS_URL <!-- replace DOCS_URL with the hosted docs address -->.

`CONTRIBUTING.md` maps the code and `docs/README.md` explains how to build the docs.

## What stays on your Mac

Your scripts, footage and exports stay on your Mac. Origami keeps:

- `origami.lock` next to each script, which records the media the script uses.
- Saved versions of each script, unsaved code kept after a crash, your export choices, the Open Recent list and your answer about reports, in `~/Library/Application Support/Origami`.
- A crash log in `~/Library/Logs/Origami`.

What it sends, in short (the [Privacy](docs/pages/00-intro.md#privacy) section has the full list):

- **Update check.** Each time it opens, Origami downloads one small file from GitHub to see if a newer version is out. It sends nothing about you or your project. Start it with `ORIGAMI_NO_UPDATE_CHECK=1` to turn this off.
- **Usage and crash reports, only if you say yes.** The launch screen asks once. Nothing is sent before you click Yes or after you click No thanks. After a yes, a few anonymous events go to PostHog, such as "app opened" and "exported", with a random install id and no script contents, file names, paths or user name. Origami › Share Usage and Crash Reports turns them off, and `ORIGAMI_NO_TELEMETRY=1` sends nothing and never asks.
- `ORIGAMI_NO_TELEMETRY` and `ORIGAMI_NO_UPDATE_CHECK` only apply when Origami is started from a terminal that sets them, such as `ORIGAMI_NO_TELEMETRY=1 /Applications/Origami.app/Contents/MacOS/origami-app`; opening it from the Dock or Finder does not read them.
- **Built from source:** a copy you build yourself never asks and never sends reports, unless you add your own PostHog key. It still checks for updates unless you turn that off.
- **Microphone:** used only while you record a voiceover.
- **Scripts** can read any file your account can read when they name it, but cannot use the network, run programs or write files other than their own `origami.lock`. Open scripts from people you trust.

## Known limits

- Apple silicon and macOS 15 or later only.
- A project is a script file (`.js`). There is no import from other editors.
- ffmpeg is not included: install it with `brew install ffmpeg`.
- Not signed with an Apple Developer ID or notarized yet. macOS asks you to allow each downloaded version once, as above, and may ask again for access to your folders and the microphone after an update.

## Report a bug

Open an issue at [github.com/DannyMang/origami/issues](https://github.com/DannyMang/origami/issues) with your macOS version, the Origami version shown on the launch screen, what you did and what happened, and the smallest script that shows it. If Origami crashed, attach `~/Library/Logs/Origami/origami.log` after checking it for anything private.

## Licence

[FSL-1.1-ALv2](LICENSE.md): use, change and share Origami for any purpose except a commercial product or service that competes with it. Each release becomes Apache-2.0 two years after it ships. The open-source crates Origami is built with are listed in [THIRD-PARTY.md](THIRD-PARTY.md).
