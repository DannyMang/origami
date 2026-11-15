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

Seconds, or a "m:ss" string.

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
