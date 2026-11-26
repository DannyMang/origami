# Origami docs

Each Markdown file in `pages/` becomes one static page in `dist/`.

## Build

```
cargo build --release --manifest-path ../origami-engine/Cargo.toml
bun install
bun run build
```

Open `dist/index.html`. Needs ffmpeg.

## Deploy

The site is static and hosted on Cloudflare at https://origami.video. Build it on a Mac, because the examples are rendered with the CLI and ffmpeg, then upload `dist/`:

```
bun run deploy
```

The site is where people download Origami and where the app checks for a newer version. The build copies `../latest.json` into `dist/`, and the deploy uploads `../dist/Origami.dmg` to the R2 bucket `origami-downloads`, because the dmg is larger than the 25 MiB a static file here may be; `worker.ts` serves any address that is not a file in `dist/` from that bucket. The deploy stops if `../dist/Origami.dmg` is missing, so run `../bundle.sh` or `../release.sh` first.

## Add a page

Add `pages/NN-name.md`. It builds to `dist/name.html`; the first file in filename order is `dist/index.html`. `#` is the page title in the left nav, `##` is a link in the "On this page" list on the right.

## Add a code block

A `js` fence is shown as code only.

    ```js
    clip.trim(90, 95)
    ```

## Add a runnable example

An `origami` fence is shown as code, rendered with the CLI, and its video is placed under the code. It must be a complete script. Paths resolve from `media/`. Everything in `media/` was generated with ffmpeg or made by the author, and is under the repository's licence.

    ```origami
    video("clips/intro.mp4").trim(0, 2)
    ```

Renders are cached in `dist/video/` by a hash of the script, everything in `media/` and the CLI binary. A failed render fails the build.

An example runs on the Mac that builds the site and can read any file there, and whatever it shows ends up in the published video. Review changes to `pages/` like code before building and deploying.

## Add a video

Image syntax with a video path under `media/`, relative to `docs/`. The file is copied into `dist/`; any other path fails the build.

    ![caption](media/clips/intro.mp4)
