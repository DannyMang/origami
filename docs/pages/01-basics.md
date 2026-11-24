# Basics

## Script

Plain JavaScript. The last expression is the video.

```origami
stack(
  video("clips/intro.mp4").trim(0, 3),
  text("hello"),
)
```

## Time

Seconds, or a "m:ss" string. `"0:05.5"` and `"1:02:03"`, with hours, work too. A time can be up to 24 hours.

```origami
const clip = video("clips/funny.mp4")
cat(
  clip.trim(1, 3),
  clip.trim("0:05", "0:07"),
)
```

## Folders

Paths are relative to the script. Build them with variables. The extension is optional.

```origami
let folder = "clips"
let file_1 = video(folder + "/intro")
file_1.trim(0, 2)
```

## settings

Sets frame rate and output size. `fps` is a whole number from 1 to 240. `size` takes numbers up to 16384. Video files need even sizes, so an odd number is rounded up and the status bar says so.

Without it, the video runs at 30 frames a second and takes the size of the first video in the script, or 1920 by 1080 when there is none.

```origami
settings({ fps: 24, size: [480, 270] })
video("clips/intro.mp4").trim(0, 2)
```

Two more keys change only the preview, never the export:

| Key | Values | Default |
|---|---|---|
| `preview` | `"auto"`, `"full"`, `"half"`, `"quarter"` | `"auto"`: as sharp as the screen shows it, at most 1920 wide while playing |
| `proxy` | `true`, `false` | `true`: while scrubbing, DJI clips show their low-resolution `.LRF` file, then the full frame when you let go |

The ⋯ menu in the timeline bar has a preview row that writes `preview` for you.

The preview is never drawn more than 8192 pixels on its long side, also with `"full"`. A bigger timeline is previewed smaller and exports at its full size.

## Limits

A script is stopped, with an error, when it runs for more than 30 seconds or uses more than 2 GB of memory. While it is still reading new media files it gets up to 5 minutes.

A timeline over 16 MB as text, which means tens of thousands of clips, or more than 1,048,576 animated values, is an error too. So are brackets nested more than 100 deep; brackets inside quoted text do not count.

## Command line

The `origami` tool runs a script without the app.

```sh
origami render edit.js out.mp4
origami frame edit.js 2.5 still.png
origami ir edit.js
```

- `render` exports the video at the timeline's size, as H.264 with sound. A frame wider or taller than 4096 pixels is saved as HEVC. It writes over `out.mp4` if it is there.
- `render --dry-run` exports nothing and prints the ffmpeg command instead.
- `frame` saves one picture, at a time in seconds.
- `ir` prints the timeline the script makes, as JSON: every clip with its start, end and values.
- `render` and `frame` refuse to write over a file the script reads.

The first run writes `origami.lock` beside the script. It lists each media file with its size, length and a fingerprint, so later runs do not read the files again. When a file has changed since, `render` and `frame` stop with `changed since origami.lock (use --relock)`. Add `--relock` to read it again and update the lock. The app and `ir` always do.
