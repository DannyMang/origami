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

## Type tool

Click the T beside the speed button, or press `t`, then click anywhere in the picture: Origami adds `text("text").at(<playhead>).for(3).pos(x, y)` to your script with its top-left corner at the spot you clicked, and you type the words right there. Enter saves them, Esc removes the new text, and the size slider, colour wheel and dragging in the preview edit it afterwards.

## Pen tool

Click the ✎ beside the T, then click points around a shape in the picture. Click the first point again, after at least three, to close it. Origami writes the shape above the script's last statement as a [path](video.html#path) named `shape`, then `shape2` and so on, as one undo step. Esc, or the ✎ again, cancels.

```js
const shape = path([
  [0.25, 0.25], [0.75, 0.25], [0.5, 0.75],
], { curve: true })
```

The shape is not shown until you use it: add it to a `stack`, or use it as a [matte](arranging.html#matte) with `clip.matte(shape)`.

Double-click the shape's name in the script to outline it on the picture until Esc. When the shape is on the timeline, the playhead also moves to where it first appears.

## Edit text on the picture

Double-click any text or caption in the preview to change its words right on the picture; press Enter (or click elsewhere) to save them into the script, Esc to cancel, and Shift+Enter for a new line. This works when the words are written as a quoted string in the `text("...")` call; for words that come from a variable, a template or a captions file, edit the script instead.

## Fonts

Click a `text(` or `caption(` call in the code, or open the clip in the Motion tab, and use the font row to choose from the font files beside your script and every font installed on this Mac; type to narrow the list, click or press Enter to apply, Esc to close. Picking writes `font: "Name"` into that call's options (adding the options if there are none, and replacing a variable in that call only), and "default" removes it.

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

## History

Every save keeps a version of the script on your Mac, so you can see what changed and bring an earlier version back.

Open the list with the ↺ button at the right end of the code pane's header, cmd+y, File › History… or `:history`. It drops down above the code, newest first, grouped by day: Today, Yesterday, then the date. Each row shows when (just now, 12 min ago, then the time), what changed, and how many lines were added (green) and removed (red). The newest is marked current unless the code on screen has unsaved changes. A change to the numbers in one call on a named clip reads like `dome · temperature -4 → -10` or `camp · trim 12, 18 → 12, 20`; any other change shows its first changed line, and a restore reads `↺ restored 11:31`.

Click a version, or move with j and k (or the arrow keys) and press enter, to compare it with the code on screen, saved or not. Red lines are only in that version, green lines came after it with their line number, the rest is dimmed, and long unchanged stretches fold to `⋯ 12 unchanged lines`. `]c` and `[c` (or ↑ and ↓ in the bar) jump between changes, `← back` or esc returns to the list, and esc again closes it.

**Restore this version** (or r) puts that version back into the script and saves it as a new version on top; the list stays open. Nothing is lost: the code that was on screen is kept as a version first, even if it was not saved, and cmd+z undoes the restore. While the file has changed on disk, pick keep mine or use disk first.

- **When.** A version is kept on every save: cmd+s, Save when Origami asks about unsaved changes, and every edit that saves by itself, such as a slider, a timeline drag or the type tool. A save that changes nothing keeps nothing. Saves less than a minute apart that change the same lines become one version, so a whole slider drag is one entry showing where it started and where it ended. Choosing use disk after the file changed on disk keeps the code on screen as a version first.
- **Where.** In `~/Library/Application Support/Origami/history`, in a folder named from the script's path, readable only by your account. Never in the project folder, so versions do not travel with a shared project. The folder records the script's path; a folder that names another script, such as one copied in by hand, is left alone, and that script keeps no versions. Each version is flushed to disk in the background right after its save, so a power cut loses at most the versions still being written.
- **Removing.** A version keeps everything that was in the script when it was saved, including text you later delete, such as a pasted password, for up to a year, and the folder records the script's path. To remove a script's versions, click clear at the bottom of the list and then remove, or type `:history clear`; this cannot be undone. To remove them by hand, quit Origami and delete its folder in `~/Library/Application Support/Origami/history` (the `path` file in each folder names its script), or the whole `history` folder.
- **How long.** Everything from the last 7 days; then the last version of each hour up to 30 days; then the last version of each day up to a year. Older versions are removed, and each script keeps at most about 50 MB, dropping the oldest first. The thinning happens the first time a script is saved after Origami opens.

## Menus

Every command is also in the menu bar, with its shortcut. File has New (cmd+n), Open… (cmd+o), Open Recent, Save (cmd+s), History… (cmd+y), Export… (cmd+shift+e) and Close Window (cmd+w). Help › What's New opens the changelog.

Export… (also ⋯ › export) opens the export dialog. It picks where to save, the resolution (the timeline's size, 4K, 1080p or 720p), the format (H.264, HEVC, or ProRes 422 saved as .mov), the quality (High, Standard or Small), the range (the whole timeline or the selected clip) and whether to include audio. Enter exports, Esc cancels, and the choices are remembered for the script in `~/Library/Application Support/Origami/export`, so they never travel with a shared project. The export is written to a hidden file beside the destination and moved into place when it finishes, so a failed or cancelled export leaves any file that was already there untouched, and the hidden file is removed even if Origami is force-quit or crashes mid-export. If the file already exists, Origami asks first: Replace writes over it, Keep Both saves the new export next to it as `name 2.mp4`, and Cancel goes back to the dialog. 4K draws text and shapes at full size, so names and captions stay sharp; video is scaled up from the clips. A size that would draw any clip over 16384 pixels on a side is refused with the clip's line; pick a smaller size or make the clip smaller. `:export name.mp4` still exports straight away at the timeline's size.
