# Arranging

## cat

Plays clips one after another.

```origami
const intro = video("clips/intro.mp4").trim(0, 2)
const middle = video("clips/funny.mp4").trim(0, 2)
const outro = text("the end").for(1)
cat(intro, middle, outro)
```

## stack

Plays clips at the same time. Later clips draw on top.

```origami
const footage = video("clips/funny.mp4").trim(0, 4)
const music = sound("music/track.mp3").trim(0, 4)
stack(footage, text("title"), music)
```

## merge

Turns a `stack` or `cat` into a group: its clips are drawn together into one picture the size of the frame, and methods on the group change that picture as a whole.

Without `merge`, a method on a `stack` or `cat` goes to each clip inside on its own. Below, both pairs of circles are made half see-through. The top pair is not merged, so where the circles overlap you see both layers. The bottom pair is merged, so it fades as one shape.

```origami
settings({ size: [640, 360] })
const apart = stack(
  rect(160, 160, { color: "#e04040", radius: 80 }).pos(0.3, 0.05),
  rect(160, 160, { color: "#e04040", radius: 80 }).pos(0.45, 0.05),
)
const merged = stack(
  rect(160, 160, { color: "#e04040", radius: 80 }).pos(0.3, 0.5),
  rect(160, 160, { color: "#e04040", radius: 80 }).pos(0.45, 0.5),
).merge()
stack(
  color("#203040").for(3),
  apart.opacity(0.5),
  merged.opacity(0.5),
)
```

- On a merged group, `scale`, `pos`, `rotate`, `opacity`, `fade`, `blend`, `matte`, `fps`, the colour methods and the [effects](methods.html#effects) change the whole picture. `scale` and `rotate` work around the middle of the frame.
- `volume` and `mute` still go to each clip inside. `at` and `shift` move the whole group.
- `for` sets how long the group lasts: clips inside that would run longer are cut at its end, sound included. `loop` repeats the whole group.
- `trim` is for one clip, and `speed` goes before `merge`, as in `stack(a, b).speed(2).merge()`, which speeds up each clip inside. On a merged group either one stops the script with an error on its line.

## matte

Shows a clip only through another clip, called its matte. A matte works like a stencil: where the matte is drawn, the clip shows; everywhere else the clip is hidden and what is behind it shows instead. The matte itself is never drawn.

```origami
settings({ size: [640, 360] })
stack(
  color("#203040").for(3),
  video("clips/funny.mp4").trim(0, 3).matte(text("WILD", { size: 200 })),
)
```

A second argument picks how the matte is read. Luma means brightness.

| Mode | The clip shows |
|---|---|
| `"alpha"` | where the matte is drawn. Half see-through parts show it half. The default |
| `"alphaInverted"` | where the matte is not drawn |
| `"luma"` | where the matte is bright: fully under white, half under grey, not at all under black or where nothing is drawn |
| `"lumaInverted"` | where the matte is dark |

```js
clip.matte(title, "alphaInverted")   // the clip everywhere except the title's letters
```

- Any clip can be a matte: text, a shape, an image, a video, or a `stack` of them. It can move and animate like any clip.
- The matte starts when its clip starts, and its keys count from there. `at` and `shift` on the matte move it later or earlier from there.
- A matte with no length of its own, such as text or a shape, lasts as long as the clip. One with its own length ends then. While the matte is not there, before it starts or after it ends, the clip is hidden, or shows in full with an inverted mode.
- A matte is never heard: a video used as a matte gives only its picture.
- The [pen tool](editor.html#pen-tool) draws a shape to use as a matte.

## blend

Sets how a clip mixes with what is under it, called its blend mode. `"normal"`, the default, covers what is under it. `"multiply"` multiplies the colours: white leaves the picture below as it is, black makes it black, and a colour tints it like coloured glass. Use it for shadows, colour washes, and paper or dirt textures.

```origami
stack(
  video("clips/funny.mp4").trim(0, 3),
  color("#ff9040").for(3).blend("multiply"),
)
```

The frame starts black, so a multiplied clip with nothing under it is black too.
