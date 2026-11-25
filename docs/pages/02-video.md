# Video

## video

Loads a video file.

```origami
video("clips/intro.mp4")
```

Playlist files (`.m3u8`, `.m3u` and ffmpeg concat lists) are not supported, in `video`, `sound` or `image`: use a video or sound file.

Origami reads H.264, HEVC (also 10-bit and HDR phone footage), ProRes, DNxHR, VP8, VP9, MPEG-2 and Motion JPEG video, GIFs, and AAC, MP3, Opus, Vorbis, FLAC, ALAC, WAV and AIFF sound. AV1 video is the exception: the app's ffmpeg cannot decode it, and the script says so on the clip's line. Convert it to H.264 or HEVC first. [Supported formats](supported-formats.html) has the whole list.

## sound

Loads an audio file.

```origami
settings({ size: [640, 360] })
sound("music/track.mp3").trim(0, 4)
```

## image

Loads a still image.

```origami
settings({ size: [640, 360] })
image("logo.png").for(3)
```

PNG, JPEG, HEIC (iPhone photos), AVIF, JPEG XL, WebP, TIFF and more work: see [Supported formats](supported-formats.html). SVG does not: export it as PNG first.

## text

Draws text, centered. Options: size, color.

```origami
settings({ size: [640, 360] })
text("summer 2026", { size: 96, color: "#ffcc00" }).for(3)
```

`font` takes a .ttf, .otf or .ttc file, or the name of an installed font. Works on `caption` too.

Text supports Latin, Cyrillic, Greek, Chinese, Japanese and Korean. Arabic, Hebrew, Indic scripts and emoji are not supported yet.

```js
text("summer 2026", { font: "fonts/Minecraft.ttf" })
text("summer 2026", { font: "Impact" })
```

`background` puts a colour plate behind the text, and `align` lines up several lines `"left"`, `"center"` or `"right"`.

```origami
settings({ size: [640, 360] })
stack(
  color("#74ade8").for(3),
  text("day one\nyosemite valley", { size: 40, background: "#0008", align: "left" }).for(3),
)
```

## caption

Draws smaller text near the bottom.

```origami
stack(
  video("clips/funny.mp4").trim(0, 4),
  caption("we made it").at(1).for(2),
)
```

## captions

Loads every caption from an .srt file. The file can be up to 16 MB and 262,144 lines.

```origami
stack(
  video("clips/funny.mp4").trim(0, 3),
  captions("subs.srt"),
)
```

## color

Fills the frame with a color.

```origami
settings({ size: [640, 360] })
color("#74ade8").for(2)
```

Colours are hex: `"#rgb"`, `"#rrggbb"`, and with transparency `"#rgba"` or `"#rrggbbaa"`. `"#0008"` is half-see-through black. Click a colour in the script to pick it on a colour wheel. Anything else, such as `"red"`, is an error on its line.

## rect

Draws a rectangle, width and height in pixels, centered. Options: color, radius.

```origami
settings({ size: [640, 360] })
stack(
  color("#203040").for(3),
  rect(400, 120, { color: "#74ade8", radius: 12 }).for(3),
)
```

## line

Draws a line from one point to another. Points are shares of the frame: `0, 0` is the top left, `1, 1` the bottom right. Options: color, width.

```origami
settings({ size: [640, 360] })
stack(
  color("#203040").for(3),
  line(0.1, 0.5, 0.9, 0.5, { color: "#ffffff", width: 4 }).for(3),
)
```

## path

Draws a filled shape through a list of points. Points are shares of the frame, as for `line`. It needs at least three points and closes itself. Options: color, and `curve: true` to round the corners into a smooth curve that still passes through every point.

```origami
settings({ size: [640, 360] })
stack(
  color("#203040").for(3),
  path([[0.1, 0.8], [0.25, 0.2], [0.4, 0.8]], { color: "#ffcc00" }).for(3),
  path([[0.6, 0.5], [0.75, 0.2], [0.9, 0.5], [0.75, 0.8]], { color: "#74ade8", curve: true }).for(3),
)
```

Where the outline crosses itself, the overlap is left empty: five points drawn as a star leave a hole in the middle.

Each number in a point also takes keys or a function of time, so the shape can change. Here the top point slides right over two seconds:

```js
path([[0.3, 0.8], [keys([0, 0.3], [2, 0.7]), 0.2], [0.7, 0.8]])
```

The [pen tool](editor.html#pen-tool) draws a path by clicking in the picture.
