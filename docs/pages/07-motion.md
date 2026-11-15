# Motion

## keys

Animates a value. Each key is `[time, value, ease]`. The ease is optional.

```origami
settings({ size: [640, 360] })
text("hello", { size: 96 }).for(3)
  .pos(keys([0, 0.05], [1.5, 0.55, "overshootOut"]), null)
  .opacity(keys([0, 0], ["30%", 1], [-0.5, 1], ["100%", 0]))
```

- Time is seconds from the clip's start (`1.5`), seconds before its end (`-0.5`), or a share of its length (`"30%"`).
- `scale`, `pos`, `opacity` and `rotate` take keys on every clip with a picture. `rotate` is in degrees, clockwise.
- Write keys in time order. Before the first key the value is the first key's; after the last, the last key's.
- `scale` or `opacity` written twice multiplies. `pos`, `rotate` and `shown` written twice replaces.

## Eases

The ease on a key shapes the move into that key. The first key's ease does nothing.

| Ease | Move |
|---|---|
| `"linear"` | steady. The default |
| `"step"` | holds, then jumps at the key |
| `"slowIn"` | starts slow |
| `"slowOut"` | ends slow |
| `"slowInSlowOut"` | starts and ends slow |
| `"springOut"` | overshoots and wobbles to rest |
| `"overshootOut"` | goes past once, then back |
| `"bounceOut"` | bounces at the end |
| `bezier(0.3, 0, 0.6, 1)` | your own curve, as in CSS `cubic-bezier`. Not quoted |

## Functions of time

Anything that takes keys also takes a function `(t, d, T) => number`: `t` is seconds into the clip, `d` the clip's length, `T` the time on the timeline.

```origami
settings({ size: [640, 360] })
text("spin", { size: 96 }).for(3).rotate(t => t * 120)
```

`delay(value, seconds)` starts keys or a function later: `delay(keys([0, 0], [0.5, 1]), 1)`.

## shown

Text and captions only. Shows the first share of the letters, from 0 to 1. Keys make a typewriter.

```origami
settings({ size: [640, 360] })
text("typing...", { size: 72 }).for(3).shown(keys([0, 0], ["80%", 1]))
```

## each

Text and captions only. Animates every `"char"`, `"word"` or `"line"` on its own. The function gets the part's number `i` and the count `n`, and returns any of `opacity`, `x`, `y`, `scale`, `rotate`.

```origami
settings({ size: [640, 360] })
text("one by one", { size: 72 }).for(3)
  .each("char", (i, n) => ({
    opacity: delay(keys([0, 0], [0.3, 1]), i * 0.08),
    y: delay(keys([0, 0.05], [0.3, 0, "slowOut"]), i * 0.08),
  }))
```

`x` and `y` move the part by fractions of the frame. `scale` and `rotate` turn it around its own centre. Each value is a number, keys or a function.

## Connections

A property can follow another clip's. Write the clip's name and the property.

```origami
const title = text("yosemite", { size: 72 }).for(4)
  .pos(keys([0, 0.05], [4, 0.4, "slowInSlowOut"]), 0.2)
const tag = text("follows", { size: 32 })
  .pos(title.pos.x, add(title.pos.y, 0.3))
  .opacity(title.opacity)
stack(video("clips/funny.mp4").trim(0, 4), title, tag)
```

- `title.pos`, `title.pos.x`, `title.pos.y`, `title.scale`, `title.rotate` and `title.opacity` can be followed. `.pos(title.pos)` follows both axes.
- `add(value, number)` adds an offset. `mul(value, number)` multiplies.
- The followed clip must be defined above, with `const`, `let` or `var`. It cannot be a group, and `each` cannot follow.
- The value is read at the same moment on the timeline, not on each clip's own clock. A clip with no `pos` gives where it sits, centred.

## Motion tab

The Motion tab sits beside Timeline in the timeline bar. Every clip is a row with a bar across its time. Click a clip, or its arrow, to open its properties. Every change rewrites the script, and cmd+z undoes one whole drag.

| Clip | Rows |
|---|---|
| Video, image, colour | pos x, pos y, scale, rotate, opacity |
| Text, caption | the same, plus shown, size, color, font, background |
| Rect | the same as video, plus width, height, radius, color |
| Line | the same as video, plus width, color |

Each row has a dot for connections, the name, the value and a key diamond.

- Drag a number sideways to change it. With keys, that sets the key at the playhead, or adds one.
- Click the diamond to add or remove a key at the playhead. Filled means animated. Removing the last key leaves a plain number. size, width, height and radius take no keys.
- Keys show as diamonds on the row. Click one to jump to it and open a box with its time, value and ease. Drag it sideways to move it. It snaps to frames, keeps how its time is written, and never passes its neighbours. Delete removes it.
- color and background open the colour control. font lists the font files next to the script or in `fonts/`.
- A value that is a function, a variable, or written twice shows `code` and cannot be dragged.
- Scroll over the names to scroll the rows. Scroll over the keys, or use the slider at the bottom, to zoom.

## Graph Editor

The switch at the bottom of the Motion tab. It draws the selected clip's animated and connected properties as curves on one graph.

- Drag a key sideways for time, up and down for value.
- A selected key with a `linear` or `bezier` ease shows handles. Dragging one writes `bezier(...)` into the ease. Pick `bezier` in the key's box to turn a named ease into one you can bend.

## Connect by drag

Drag a row's dot onto another clip's row to make it follow that row. Rows it cannot follow fade while you drag. Or click the dot to pick from a list of the named clips above.

A connected row shows a chip such as `← title.pos y`. Click it to go to the source, break the link, or add an offset or multiplier. Breaking writes the current value back as a plain number. Drag the offset sideways to change it.

## Graph tab

A picture of what follows what. One box per clip lists its animated, connected and followed properties. A line runs from each source to its follower, which sits one column to the right. Click a box to select the clip and jump to its line. Read-only.

## Drag in the preview

Click text, a shape, or a video or image that is scaled or placed, to select it. Drag it to move it.

- With no keys, the drag rewrites `.pos(x, y)`. With keys, it sets the key at the playhead, or adds one. A connected axis stays as it is.
- The selected clip shows a thin box. When its position has keys, a dotted line shows its path, with a dot for each key.
