# Methods

## trim

Keeps the source between two times.

```origami
video("clips/funny.mp4").trim("0:02", "0:05")
```

## for

Sets how long the clip lasts.

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

Plays faster or slower.

```origami
video("clips/funny.mp4").trim(0, 6).speed(2)
```

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

## loop

Repeats the clip.

```origami
video("clips/funny.mp4").trim(0, 1).loop(3)
```
