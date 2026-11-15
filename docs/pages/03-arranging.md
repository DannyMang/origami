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
