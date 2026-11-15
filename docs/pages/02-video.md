# Video

## video

Loads a video file.

```origami
video("clips/intro.mp4")
```

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

## text

Draws text, centered. Options: size, color.

```origami
settings({ size: [640, 360] })
text("summer 2026", { size: 96, color: "#ffcc00" }).for(3)
```

`font` takes a .ttf, .otf or .ttc file, or the name of an installed font. Works on `caption` too.

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

Loads every caption from an .srt file.

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
