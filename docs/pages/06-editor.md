# Editor

## Autocomplete

A list of matching names opens under the caret as you type. It lists only what Origami has.

- After a dot on a clip: methods (`trim`, `speed`, `temperature`, ...).
- At the start of a word: clip functions (`video`, `sound`, `text`, `cat`, `stack`, ...) and the variables defined in the file.
- Inside the quotes of `video("`, `sound("`, `image("`, `captions("`: files in that folder.
- Inside the quotes of `look("` and `inputLut("`: `.cube` files and look names.

Keys: up and down move, enter or tab accepts, esc closes.

```js
clip.te
//    temperature  amount
```

## Click controls

Hover a call that takes numbers. It underlines. Click it to open its controls. Dragging rewrites the numbers in the script and the preview follows. Esc or a click outside closes it. cmd+z undoes one whole drag.

| Value | Control | Calls |
|---|---|---|
| Number | slider | `speed`, `volume`, `scale`, `loop`, `pos`, `fade`, `trim`, `for`, `at`, `shift`, every colour slider |
| Hue and amount | colour wheel, plus sliders | `shadowTint`, `highlightTint`, `wheel` |
| Points | curve pad, plus sliders | `curve`, `hueVsSat`, `hueVsHue`, `hueVsLuma`, `lumaVsSat`, `satVsSat` |
| Colour | colour wheel and a light slider | `color`, text and caption `color`, `hslSecondary` key |

```js
clip.temperature(-10)
//   click, then drag the slider from -100 to 100
```

- A time written as `"0:10"` becomes seconds once you drag it.
- A value that is a variable or a sum gets no control.
- Curve pads are coloured by what they act on: a hue strip for the hue curves, grey for luma, grey to vivid for saturation, and red, green or blue for those channels.
- Not built yet: adding or removing curve points by click, picking a colour from the preview, a frame scrubber for times.

## Problems

A value that cannot work gets a red squiggle. Hover it to read why.

```js
clip.trim(5, 2)          // trim start must be before its end
clip.temperature(250)    // temperature takes -100 to 100
clip.trim(0, 99)         // the file is only 39.67 s long
```

A bracket with no partner is marked as you type.

```js
stack(                   // this ( is never closed
```

A file that does not exist, or a line that fails when the script runs, is marked on save.

## Hover info

Rest the pointer on a name to see what it is.

- A method or function: its arguments and what it does.
- A variable: its kind and what it accepts.

| Kind | Made by | Accepts |
|---|---|---|
| Video | `video` | timing, sound, placement, colour |
| Audio | `sound` | timing and sound. No picture, so no colour |
| Shape | `text`, `caption`, `captions`, `image`, `color` | timing, placement, colour. No source to trim |
| Group | `cat`, `stack` | everything; it applies to each clip inside |

Autocomplete only offers the methods a kind accepts.

## Search

cmd+f or ctrl+f opens search in the code pane. Typing jumps to the first match. Enter goes to the next, shift+enter to the previous, esc closes.

When the text is exactly a clip's name, the timeline also jumps to that clip.

```js
clip_6
// selects clip_6 in the code and moves the playhead to it
```

## Folding

Click a gutter arrow to hide a block or a run of definitions. cmd+opt+[ folds at the caret, cmd+opt+] unfolds.

## Layout

Drag the edge between panes to resize. cmd+= and cmd+- change the code size. cmd+0 resets.

## Menus

Every command is also in the menu bar, with its shortcut. File has New (cmd+n), Open… (cmd+o), Open Recent, Save (cmd+s), Export… (cmd+shift+e, the same as `:export`) and Close Window (cmd+w). Help › What's New opens the changelog.
