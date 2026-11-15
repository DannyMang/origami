# Color

Names, ranges and defaults follow Lumetri Color. The maths is Origami's own, so a value will not look exactly as it does in Premiere. Preview and export use the same numbers.

Order of steps: inputLut, basic sliders, look, creative sliders, curves, wheels, hslSecondary, vignette. Call order in the script does not change it. Calling a method again replaces its earlier value. On a group, a method applies to every clip inside.

## temperature

Cooler (negative, blue) or warmer (positive, orange).

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).temperature(-40)
```

## tint

Green (negative) or magenta (positive).

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).tint(-40)
```

## saturation

Overall colour strength. 0 is black and white.

Range: 0 to 200, default 100.

```origami
video("clips/funny.mp4").trim(0, 2).saturation(40)
```

## exposure

Overall brightness, in stops.

Range: -5 to 5, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).exposure(-1)
```

## contrast

Pushes midtones apart or together.

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).contrast(-60)
```

## highlights

Brightens or darkens bright areas.

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).highlights(-80)
```

## shadows

Brightens or darkens dark areas.

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).shadows(80)
```

## whites

Sets the white clipping point.

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).whites(-60)
```

## blacks

Sets the black clipping point.

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).blacks(60)
```

## look

Applies a look from a .cube file. Second argument is intensity. A bare name is `name.cube` next to the script, then a look from an installed Premiere Pro.

Range: intensity 0 to 200, default 100.

```origami
video("clips/funny.mp4").trim(0, 2).look("looks/teal-orange.cube", 150)
```

```js
clip.look("Kodak 5205 Fuji 3510 (by Adobe)", 80)
```

## inputLut

Applies a technical LUT before every other colour step.

Range: file path.

```origami
video("clips/funny.mp4").trim(0, 2).inputLut("looks/gamma.cube")
```

## fadedFilm

Lifts blacks and lowers whites.

Range: 0 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).fadedFilm(80)
```

## sharpen

Sharpens (positive) or softens (negative). Not built yet: accepted, changes nothing.

Range: -100 to 100, default 0.

```js
clip.sharpen(15)
```

## vibrance

Saturation that favours dull colours.

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2).vibrance(-80)
```

## shadowTint

Tints the shadows. Hue in degrees, amount 0 to 1.

Range: hue 0 to 360, amount 0 to 1.

```origami
video("clips/funny.mp4").trim(0, 2).shadowTint({ hue: 210, amount: 0.8 })
```

## highlightTint

Tints the highlights.

Range: hue 0 to 360, amount 0 to 1.

```origami
video("clips/funny.mp4").trim(0, 2).highlightTint({ hue: 40, amount: 0.8 })
```

## tintBalance

Moves the split between shadow tint and highlight tint. Positive gives the highlight tint more room.

Range: -100 to 100, default 0.

```origami
video("clips/funny.mp4").trim(0, 2)
  .shadowTint({ hue: 210, amount: 0.8 })
  .highlightTint({ hue: 40, amount: 0.8 })
  .tintBalance(-80)
```

## curve

Tone curve on luma, red, green or blue. Points are [input, output] from 0 to 1. One curve per channel.

Range: channel, list of points.

```origami
video("clips/funny.mp4").trim(0, 2)
  .curve("luma", [[0, 0], [0.5, 0.7], [1, 1]])
  .curve("red", [[0, 0], [0.5, 0.3], [1, 1]])
```

## hueVsSat

Changes saturation for a range of hues. Points are [hue 0 to 1, change]. -1 removes the colour.

Range: list of points.

```origami
video("clips/funny.mp4").trim(0, 2).hueVsSat([[0.2, 0], [0.33, -1], [0.45, 0]])
```

## hueVsHue

Shifts a range of hues to another hue. A change of 1 is a full turn.

Range: list of points.

```origami
video("clips/funny.mp4").trim(0, 2).hueVsHue([[0.2, 0], [0.33, 0.3], [0.45, 0]])
```

## hueVsLuma

Changes brightness for a range of hues.

Range: list of points.

```origami
video("clips/funny.mp4").trim(0, 2).hueVsLuma([[0.2, 0], [0.33, -0.4], [0.45, 0]])
```

## lumaVsSat

Changes saturation by brightness.

Range: list of points.

```origami
video("clips/funny.mp4").trim(0, 2).lumaVsSat([[0.4, 0], [0.7, -1]])
```

## satVsSat

Changes saturation by existing saturation.

Range: list of points.

```origami
video("clips/funny.mp4").trim(0, 2).satVsSat([[0.5, -1], [0.9, 0]])
```

## wheel

Colour offset in shadows, midtones or highlights, with a brightness offset. One wheel per range.

Range: hue 0 to 360, amount 0 to 1, luma -1 to 1.

```origami
video("clips/funny.mp4").trim(0, 2)
  .wheel("shadows", { hue: 210, amount: 0.6, luma: -0.1 })
  .wheel("highlights", { hue: 40, amount: 0.4, luma: 0.1 })
```

## hslSecondary

Corrects one colour range only. Pick the range with key, change it with correction (temperature, tint, contrast, saturation). Not built yet: refine and correction.sharpen are accepted and change nothing.

Range: key color, range 0 to 100 (default 20), soft 0 to 100 (default 10).

```origami
video("clips/funny.mp4").trim(0, 2).hslSecondary({
  key: { color: "#30e030", range: 20, soft: 10 },
  refine: { denoise: 10, blur: 4 },
  correction: { temperature: -10, saturation: 0 },
})
```

## vignette

Darkens (negative) or lightens (positive) the edges. Roundness below 0 is boxier, above 0 is a circle.

Range: amount -3 to 3 (default 0), midpoint 0 to 100 (default 50), roundness -100 to 100 (default 0), feather 0 to 100 (default 50).

```origami
video("clips/funny.mp4").trim(0, 2).vignette({ amount: -2.5, midpoint: 40, roundness: 0, feather: 40 })
```

## Not built

sharpen, hslSecondary refine (denoise, blur) and hslSecondary correction.sharpen are accepted but do not change the picture yet.

HDR controls, Color Match, Auto, the Settings tab, and exporting a grade as a LUT do not exist.
