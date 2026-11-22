# Methods

## trim

Keeps the source between two times. It works on one clip: on a `stack` or `cat` it is an error, so trim the clips inside.

```origami
video("clips/funny.mp4").trim("0:02", "0:05")
```

## for

Sets how long the clip lasts. On a `stack` or `cat`, clips inside that would run longer are cut at its end, sound included.

```origami
settings({ size: [640, 360] })
text("hi").for(3)
```

## at

Sets when the clip starts inside its group.

```origami
stack(
  video("clips/intro.mp4").trim(0, 4),
  text("hi").at("0:02"),
)
```

## shift

Moves the clip later, or earlier if negative.

```origami
cat(
  video("clips/intro.mp4").trim(0, 3),
  text("early").for(2).shift(-1),
)
```

## speed

Plays faster or slower. On a `stack` or `cat`, each clip inside plays faster or slower on its own. A [merged group](arranging.html#merge) takes it before `merge`: `stack(a, b).speed(2).merge()`.

```origami
video("clips/funny.mp4").trim(0, 6).speed(2)
```

## fps

Plays the clip at its own, lower frame rate. Each picture is held until the next, so movement steps instead of gliding, like stop motion. Its video, keys, fades and effects all step; its sound does not.

Range: above 0, up to 240. A number above the video's frame rate changes nothing.

```origami
settings({ size: [640, 360] })
stack(
  color("#203040").for(2),
  text("smooth", { size: 48 }).for(2).pos(keys([0, 0], [2, 0.7]), 0.2),
  text("stepped", { size: 48 }).for(2).pos(keys([0, 0], [2, 0.7]), 0.6).fps(6),
)
```

This is not `settings({ fps })`, which sets the frame rate of the whole video.

## volume

Scales loudness.

```origami
settings({ size: [640, 360] })
const music = sound("music/track.mp3").trim(0, 3)
music.volume(0.4)
```

## mute

Silences the clip.

```origami
video("clips/intro.mp4").trim(0, 3).mute()
```

## fade

Fades in and out, in seconds.

```origami
video("clips/funny.mp4").trim(0, 4).fade(1, 2)
```

## scale

Resizes the picture.

```origami
video("clips/intro.mp4").trim(0, 2).scale(0.5)
```

## pos

Places the top-left corner, as fractions of the frame. null centers that axis.

```origami
const clip = video("clips/intro.mp4").trim(0, 3)
stack(
  video("clips/funny.mp4").trim(0, 3),
  clip.scale(0.3).pos(0.65, 0.05),
  text("low").pos(null, 0.8),
)
```

## opacity

How solid the picture is: 1 is solid, 0.5 half see-through, 0 invisible. Takes [keys](motion.html#keys).

```origami
stack(
  video("clips/funny.mp4").trim(0, 3),
  video("clips/intro.mp4").trim(0, 3).scale(0.5).opacity(0.5),
)
```

## rotate

Turns the picture around its centre, in degrees: positive is clockwise, negative the other way. Takes [keys](motion.html#keys).

```origami
settings({ size: [640, 360] })
text("tilted", { size: 96 }).for(3).rotate(-10)
```

## loop

Repeats the clip. On a `stack` or `cat`, the whole group repeats.

```origami
video("clips/funny.mp4").trim(0, 1).loop(3)
```

## Effects

The next seven methods change a clip's picture: `lens`, `blur`, `levels`, `noise`, `fractal`, `roughen` and `displace`. They work on video, image, text, caption, color, rect, line, path and a [merged group](arranging.html#merge).

- They run in the order you write them, after the colour methods, on the clip's own picture before it is placed and turned.
- Calling one twice applies it twice: `.blur(4).blur(4)` is blurrier than `.blur(4)`.
- Sizes are in pixels of the finished video. An export at a bigger size, or a smaller preview, scales them to match.
- An effect stays inside the clip's own picture. Text and shapes are only as big as they look, so a blur or a wobble stops in a hard line at their box. Put them in a merged `stack` to give the effect the whole frame: `stack(text("glow")).merge().blur(10)`.
- On a `stack` or `cat` that is not merged, each clip inside gets the effect on its own.
- Values marked "takes keys" also take keys or a function of time. The others take a plain number.
- A number outside an effect's range is brought to the nearest value inside it, and the status bar says so. Keys and functions of time for `lens` are held inside its range the same way.
- Not yet: effects do nothing on text or captions that use `shown` or `each`.

## lens

Bends the picture as a wide lens would. The number is the lens's field of view in degrees: how wide an angle it sees. A positive number curves the picture into a barrel, like a fisheye lens: it shrinks toward the middle, more at the edges, and the corners turn see-through. A negative number pinches the other way: the edges stretch outward past the frame.

Range: -170 to 170, 0 changes nothing. 30 is gentle, 90 strong. Takes keys.

```origami
video("clips/funny.mp4").trim(0, 3).lens(keys([0, 0], [3, 90]))
```

## blur

Softens the picture. The number is the radius in pixels: how far each pixel spreads. The clip's own edges blur too and turn see-through, so what is behind it shows at the border.

Range: 0 and up, 0 changes nothing. 2 to 30 is typical. Takes keys.

```origami
video("clips/funny.mp4").trim(0, 3).blur(keys([0, 0], [3, 20]))
```

## levels

Stretches the tones between two points, `black` and `white`, each a share from 0 to 1. Everything at or below `black` turns black, everything at or above `white` turns white, and the rest is spread evenly between. Moving them toward each other adds contrast. `white` must be above `black`: one at or below it is raised to just above it.

Range: black and white 0 to 1, white above black. Both are needed.

```origami
video("clips/funny.mp4").trim(0, 3).levels(0.2, 0.8)
```

A third argument, `"alpha"`, does the same to how solid the picture is instead of its colours. After a blur this turns soft edges back into clean, rounded ones, so shapes that come close melt into one:

```origami
settings({ size: [640, 360] })
const drops = stack(
  rect(120, 120, { radius: 60 }).pos(keys([0, 0.15], [3, 0.42]), 0.33),
  rect(120, 120, { radius: 60 }).pos(keys([0, 0.67], [3, 0.4]), 0.33),
).merge()
stack(
  color("#203040").for(3),
  drops.for(3).blur(16).levels(0.45, 0.55, "alpha"),
)
```

## noise

Adds grain: each pixel gets lighter or darker by a random amount, the same for red, green and blue. The first number is the strength. The second, the seed, picks the random pattern. With a fixed seed the grain stands still, like dust on the lens. Give the seed a function of time so the grain changes every frame, like film: at 30 frames a second, `t => t * 30` gives each frame a new seed.

Range: amount 0 to 1, about 0.05 to 0.2 for grain. Seed: default 0, takes keys, rounded down to a whole number.

```origami
video("clips/funny.mp4").trim(0, 3).noise(0.12, t => t * 30)
```

## fractal

Replaces the picture's colours with soft grey clouds, made of noise at several sizes. The clip keeps its shape: text stays text, filled with clouds. It is meant as a texture: blend it with `"multiply"`, or use it as a `"luma"` [matte](arranging.html#matte).

Options: `scale` is the size of the biggest clouds in pixels, 1 or more, default 100. `seed` picks the pattern, default 0. The seed takes keys and is rounded down to a whole number, so the clouds jump to a new pattern rather than drift.

```origami
stack(
  video("clips/funny.mp4").trim(0, 3),
  color("#ffffff").for(3).fractal({ scale: 200 }).blend("multiply"),
)
```

## roughen

Eats into the picture's edges with a ragged line, like torn paper or ink. The edges are the outline of text and shapes, and the border of a video or image.

| Option | What it does | Default |
|---|---|---|
| `border` | how far into the picture the roughness reaches, in pixels. 0 or more | 8 |
| `scale` | the size of the bumps: 100 is about 10 pixels, bigger is coarser. 1 or more | 100 |
| `sharpness` | how crisp the torn edge is: 0 is a soft fade with no bumps, 3 to 5 is crisp. 0 or more | 1 |
| `seed` | picks the pattern. Takes keys, rounded down to a whole number | 0 |

```origami
settings({ size: [640, 360] })
stack(
  color("#f0e8d8").for(3),
  text("rough", { size: 140, color: "#c03030" }).for(3).roughen({ border: 6, scale: 40 }),
)
```

## displace

Pushes every pixel a short way in a direction that changes smoothly across the picture, so it ripples like heat haze or water.

| Option | What it does | Default |
|---|---|---|
| `amount` | how far pixels move, up to this many pixels each way. Takes keys | 50 |
| `size` | the size of the ripples, in pixels. 1 or more | 100 |
| `seed` | picks the pattern. Takes keys, rounded down to a whole number. Change it over time to make the ripples move | 0 |

```origami
settings({ size: [640, 360] })
stack(
  color("#203040").for(3),
  text("wobble", { size: 120 }).for(3)
    .displace({ amount: keys([0, 0], [3, 12]), size: 40, seed: t => t * 12 }),
)
```
