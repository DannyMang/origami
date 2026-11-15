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

## Requirements

- A Mac with Apple silicon (M1 or later)
- ffmpeg: `brew install ffmpeg`
- Rust, to build from source

## Run

```
cargo run --release --manifest-path origami-app/Cargo.toml -- path/to/edit.js
```

With no script it opens the launch screen. The `origami` CLI in `origami-engine` renders a script without the app: `origami render edit.js out.mp4`.

## Docs

The language reference and changelog live in `docs/pages`. `docs/README.md` explains how to build and deploy them. `CONTRIBUTING.md` maps the code.
