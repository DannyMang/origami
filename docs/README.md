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

The site is static and hosted on Cloudflare at https://origami-docs.veri-web.workers.dev. Build it on a Mac, because the examples are rendered with the CLI and ffmpeg, then upload `dist/`:

```
bun run deploy
```

## Add a page

Add `pages/NN-name.md`. It builds to `dist/name.html`; the first file in filename order is `dist/index.html`. `#` is the page title in the left nav, `##` is a link in the "On this page" list on the right.

## Add a code block

A `js` fence is shown as code only.

    ```js
    clip.trim(90, 95)
    ```

## Add a runnable example

An `origami` fence is shown as code, rendered with the CLI, and its video is placed under the code. It must be a complete script. Paths resolve from `media/`.

    ```origami
    video("clips/intro.mp4").trim(0, 2)
    ```

Renders are cached in `dist/video/` by a hash of the script, everything in `media/` and the CLI binary. A failed render fails the build.

## Add a video

Image syntax with a video path, relative to `docs/`. The file is copied into `dist/`.

    ![caption](media/clips/intro.mp4)
