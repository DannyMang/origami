# Contributing to Origami

Origami is a programmatic video editor: you describe an edit in a small JavaScript script, and the app previews and exports it.
This file is the map of the project for people and coding agents.

## Layout

Three Rust crates sit side by side. Each has its own `Cargo.toml` and `Cargo.lock`; there is no workspace.
The later crates depend on the earlier ones by relative path.

| Crate | What it does | Depends on |
| --- | --- | --- |
| `origami-lang` | Runs the script and returns a `Timeline` | nothing local |
| `origami-engine` | Preview frames, audio playback, ffmpeg export, the `origami` CLI | `origami-lang` |
| `origami-app` | The desktop app (GPUI) | both |

## How data flows

1. `origami_lang::eval(script)` runs the user's script in Boa with `prelude.js` loaded and returns a `Timeline`: fps, size, duration and a flat list of `Clip`s with absolute start and end times.
2. `origami-engine` turns a `Timeline` into pixels and sound. `Preview::frame(&timeline, t)` composites one preview frame, `Player` plays audio, `render` exports through ffmpeg.
3. `origami-app` watches the script file, re-evaluates it when it changes, and draws the preview, timeline and code pane. Mouse edits in the app rewrite the script text and save it; the file is always the source of truth.

## Where things live

### origami-lang/src

| File | Contents |
| --- | --- |
| `lib.rs` | `eval`, media probing and `origami.lock` |
| `ir.rs` | `Clip`, `Timeline` and the other types a script turns into |
| `prelude.js` | The script API: `video`, `sound`, `text`, `cat`, `stack` and every clip method |
| `disk.rs` | `save` and `load`: whole-file saves that never cut a script short |
| `grade.rs` | Colour grade types carried on a clip |
| `error.rs` | Errors a script author can fix, each with the script line |
| `tool.rs` | Finds `ffmpeg` and `ffprobe`: next to the program, then `PATH`, then the usual Homebrew and MacPorts folders |
| `picture.rs` | Reads HEIC, AVIF, JPEG XL, WebP and rotated JPEG pictures with macOS ImageIO, in a copy of the program started for each picture so a bad file cannot crash the app |

### origami-engine/src

| File | Contents |
| --- | --- |
| `preview.rs` | Frame compositing, ffmpeg decoders, frame cache, thumbnails, waveform peaks |
| `audio.rs` | `Player`: ffmpeg audio mix played through cpal |
| `export.rs` | ffmpeg filter graph for export and for preview audio |
| `pixels.rs` | Drawing one layer onto the frame: scaled, turned, blended, nudged |
| `place.rs` | Where and when a clip shows: size, position, fades |
| `timing.rs` | Which source frame a timeline frame shows, and where a clip can be split without changing any |
| `grade.rs`, `lut.rs` | Colour pipeline baked to a 3D LUT, `.cube` reading and writing, vignette mask |
| `text.rs` | Text rasterising |
| `main.rs` | `origami render` and `origami ir` commands |

### origami-app/src

| File | Contents |
| --- | --- |
| `main.rs` | The `Origami` state struct, the 60 Hz `tick`, script loading, preview frame requests |
| `view.rs` | All rendering: preview stage, timeline, code pane, status bar, launch screen |
| `mouse.rs` | Scrub, clip drag and trim, zoom, pane resize, opening a script |
| `menu.rs` | Menu bar, its actions and key bindings, New/Open/Open Recent, files opened from Finder, quitting |
| `keys.rs` | Key handling, vim mode, `:` commands, script rewrites |
| `edit.rs` | Code pane text editing, selection, undo and redo |
| `complete.rs` | Autocomplete and the list of script functions (`DEFS`) |
| `knob.rs` | Sliders, colour wheel and curve editor for numbers in the script |
| `lint.rs` | Inline warnings and hover tips |
| `fold.rs`, `search.rs` | Code folding and find |
| `shelf.rs` | History store: a version of the script on every save, written off the main thread to `~/Library/Application Support/Origami/history` under a name made from the script's path, thinning and the line diff |
| `saves.rs` | History panel: the list, the diff against the code on screen and restore |
| `pulse.rs` | Opt-in anonymous usage and crash reports: the question, the stored answer and install id, the events, the crash scrubber |
| `crash.rs` | Panic hook: saves unsaved code to `~/Library/Application Support/Origami/recovered`, under a name made from the script's path, and logs to `~/Library/Logs/Origami/origami.log`; a quit that could not ask saves it there too |

## Build, run, test

You need Rust (edition 2024), `ffmpeg` and `ffprobe` on your PATH, and macOS (GPUI, VideoToolbox, system fonts). Only the app that `./bundle.sh` builds carries its own ffmpeg: `cargo run` and the tests use the one on your PATH, such as Homebrew's, and pass with either.

```
cargo run --release --manifest-path origami-app/Cargo.toml -- path/to/edit.js
cargo run --release --manifest-path origami-engine/Cargo.toml -- render path/to/edit.js out.mp4
cargo test --manifest-path origami-lang/Cargo.toml
```

Running the app with no argument opens the launch screen. Run `cargo test` and `cargo fmt` in each crate you touch; a build must produce zero warnings.
Set `ORIGAMI_NO_UPDATE_CHECK` (any value) to stop the app checking for a newer version at startup.
A build from source sends no usage or crash reports and never asks to, unless `ORIGAMI_POSTHOG_KEY` is set when it is compiled; `./bundle.sh` reads it from a gitignored `.env` at the repo root when that file exists.

`./bundle.sh` builds `dist/Origami.app` and `dist/Origami.dmg`, with `LICENSE` and `THIRD-PARTY.md` in the app's Resources. It first runs `./ffmpeg.sh`, which downloads the FFmpeg release named at its top from ffmpeg.org, refuses it unless it has the SHA-256 written there, and builds `ffmpeg` and `ffprobe` from it for Apple silicon with no outside libraries, so they are under the LGPL and link only to macOS itself; this takes a minute or two the first time and nothing after that, until the version or the configure flags in the script change. The two programs, the source file and the configure line are kept in the git-ignored `ffmpeg` folder, the programs are copied into the app next to `origami-app`, and the source file is copied to `dist` beside the dmg. To move to another FFmpeg release, change `VERSION` and `SHA256` in `ffmpeg.sh` and the ffmpeg paragraph of `THIRD-PARTY.md`, which `./bundle.sh` checks against what was built. Without `SIGN_ID` the app is signed ad-hoc, which runs on any Apple silicon Mac once its owner allows it in Privacy & Security; `SIGN_ID="Developer ID Application: …"` with `NOTARY_PROFILE=<notarytool keychain profile>` signs with that identity, notarises and staples instead. Both use the hardened runtime and `origami.entitlements`.

`./release.sh <version>` checks the tree, sets the version in the three `Cargo.toml` files and `latest.json`, dates the changelog's "(not yet released)" heading, runs the checks and `./bundle.sh`, and prints the dmg's SHA-256 and the commands to publish it: the `git` and `gh` ones for the tag and release, which attach FFmpeg's source file to the release next to the dmg as its licence asks, and the docs deploy, which puts `dist/Origami.dmg` and `latest.json` on origami.video, where the app looks for updates. `--dry-run` only checks and prints. To tell older versions they should update, add `"minimum": "<oldest supported version>"` to `latest.json`: a version below it says so in an alert each time it opens, with Download and Later, and keeps working. Update `THIRD-PARTY.md` when a `Cargo.lock` gains or changes a crate.

## House style

- No comments and no doc comments. Names carry the meaning.
- No code that is not used today: no options, traits, generics or builders for a future case.
- One file per concern until it passes about 300 lines. No `mod.rs` trees.
- Prefer a function to a type, a match to a trait, a tuple to a struct used once.
- Errors are `anyhow::Result` with `?`. A failure the script author can fix gets a variant in `origami-lang/src/error.rs` that names the script line.
- Add a dependency only if it removes more code than it adds. Shell out to ffmpeg instead of binding it.
- Test parsing and timeline math only.
- If a change makes something unused, delete it in the same change.

## Common changes

- **New clip method** (for example a new grade control): add it to `prelude.js`, carry the value on `Clip` or `Grade` in `origami-lang`, apply it in `origami-engine`, then add it to `DEFS` in `complete.rs` and, if it takes a number, a range in `knob.rs`.
- **New app interaction**: state goes on `Origami` in `main.rs`, input handling in `mouse.rs` or `keys.rs`, drawing in `view.rs`.
- **GPUI APIs**: the version is pinned to 0.2.2. Check the crate source before using a method; do not guess.
