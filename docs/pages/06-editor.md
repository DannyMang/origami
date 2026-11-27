# Editor

## Autocomplete

A list of matching names opens under the caret as you type. It lists only what Origami has.

- After a dot on a clip: methods (`trim`, `speed`, `temperature`, ...).
- At the start of a word: clip functions (`video`, `sound`, `text`, `cat`, `stack`, ...) and the variables defined in the file.
- Inside the quotes of `video("`, `sound("`, `image("`, `captions("`: files in that folder.
- Inside the quotes of `look("` and `inputLut("`: `.cube` files and look names.
- Inside the quotes of `font: "`: the font files beside the script or in `fonts/`, and every installed font. Inside `align: "`: `left`, `center` and `right`.
- After a clip's name and a dot inside `pos(`, `scale(`, `rotate(`, `opacity(`, `shown(`, `add(` or `mul(`: the properties that can be [followed](motion.html#connections), such as `pos.x` and `opacity`.

Keys: up and down move, enter or tab accepts, esc closes.

```js
clip.te
//    temperature  amount
```

## Click controls

Hover a call that takes numbers. It underlines. Click it to open its controls. Dragging rewrites the numbers in the script and the preview follows. Esc or a click outside closes it. cmd+z undoes one whole drag.

| Value | Control | Calls |
|---|---|---|
| Number | slider | `speed`, `volume`, `scale`, `opacity`, `rotate`, `shown`, `pos`, `trim`, `fade`, text and caption `size`, `look` intensity, every colour slider, `displace` amount (0 to 100 pixels) and size |
| Hue and amount | colour wheel, plus sliders | `shadowTint`, `highlightTint`, `wheel` |
| Points | curve pad, plus sliders | `curve`, `hueVsSat`, `hueVsHue`, `hueVsLuma`, `lumaVsSat`, `satVsSat` |
| Colour | colour wheel, a light slider and a strength slider for how solid it is | any hex colour: `color`, the `color` and `background` options, `duotone`, `hslSecondary` key |

```js
clip.temperature(-10)
//   click, then drag the slider from -100 to 100
```

- A time written as `"0:10"` becomes seconds once you drag it.
- A value that is a variable or a sum gets no control. Nor do keys, or a value that follows another clip.
- `trim` slides up to the length of the file and `fade` up to the length of the clip, so they need a clip that is on the timeline.
- `loop`, `for`, `at` and `shift` have no range to slide over, so they get no control. Type the number, or drag the clip on the [timeline](#timeline).
- On a curve pad, drag a point to move it, click empty space to add one, and double-click a point to remove it.
- Curve pads are coloured by what they act on: a hue strip for the hue curves, grey for luma, grey to vivid for saturation, and red, green or blue for those channels.
- Not built yet: picking a colour from the preview, a frame scrubber for times.

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

A file that does not exist, or a line that fails when the script runs, is marked on save. The status bar shows the error with its line.

```js
clip.tirm(0, 1)          // tirm is not a clip method: check the spelling, or the script may need a newer Origami
```

## Hover info

Rest the pointer on a name to see what it is.

- A method or function: its arguments and what it does.
- A variable: its kind and what it accepts.

| Kind | Made by | Accepts |
|---|---|---|
| Video | `video` | timing, sound, placement, colour |
| Audio | `sound` | timing and sound. No picture, so no colour |
| Shape | `image`, `color`, `rect`, `line`, `path` | timing, placement, colour. No source to trim |
| Text | `text`, `caption`, `captions` | the same as a shape, plus `shown` and `each` |
| Group | `cat`, `stack` | everything; it applies to each clip inside. Only a group takes `merge` |

Autocomplete only offers the methods a kind accepts.

- A video also shows its length, its size and whether it has sound. A sound shows its length.
- On `title.pos`, `title.scale`, `title.rotate` or `title.opacity` given as a value, it says that the value [follows](motion.html#connections) that clip.
- Triple-click a line that loads or uses a video or sound to open a small player for it: a play button and a bar to drag. Esc closes it.

## Search

cmd+f or ctrl+f opens search in the code pane. Typing jumps to the first match. Enter goes to the next, shift+enter to the previous, esc closes.

- cmd+g, the down arrow and the › button also go to the next match. The up arrow and ‹ go to the previous. The bar shows which match you are on, such as 2/7.
- Three switches narrow the search: `Aa` matches capitals exactly, `wd` matches whole words only, and `.*` reads the text as a regular expression. A pattern that cannot be read turns red.
- Click in the code to type there while the bar stays open. Click the bar, or press cmd+f, to type in the search again.

When the text is exactly a clip's name, the timeline also jumps to that clip.

```js
clip_6
// selects clip_6 in the code and moves the playhead to it
```

## Folding

Click a gutter arrow to hide a block or a run of definitions. cmd+opt+[ folds at the caret, cmd+opt+] unfolds.

## Layout

Drag the edge between panes to resize. cmd+= and cmd+- change the code size. cmd+0 resets the panes, the code size and the timeline's zoom.

The ⋯ menu in the timeline bar has the same as buttons: zoom in, zoom out, full screen, bigger code, smaller code, reset layout, export, and the preview row.

When the left pane is too narrow for the whole timeline bar, the bar leaves out what does not fit, in this order: the timecode (the status bar still shows it), then undo, redo, delete and the microphone, then the speed, T, ✎ and +. The tabs, the transport buttons, the record button and ⋯ always stay, and everything comes back when the pane is wide enough.

## Timeline

The Timeline tab shows every clip as a bar: pictures in the lanes above, sound below. A video with sound has a bar in both.

- Drag the ruler to move the playhead, or double-click empty space.
- Click a clip to select it. The code scrolls to its line. Clicking a line of code moves the playhead to the clip on that line.
- Drag a clip to move it. This writes `.shift(...)`. It snaps to the edges of other clips, the start and the playhead.
- Drag an edge to trim it. This writes `.trim(...)` on a video or sound and `.for(...)` on anything else. A clip is never shorter than 0.04 seconds.
- Scroll to zoom, and scroll sideways to move along. When there are more lanes than fit, scrolling moves through the lanes and cmd+scroll zooms.
- The ↩, ↪ and ✕ buttons in the timeline bar undo, redo and delete the selected clip.
- The + menu adds `text("text")` or `caption("caption")` at the playhead for 3 seconds, with the caret in the words so you can type over them.

Right-click a clip (a two-finger click) for a menu. The keys work on the selected clip while you are not typing in the code.

| Item | Key | What it does |
|---|---|---|
| Split at playhead | s | cuts a video or sound in two at the playhead |
| Cut | cmd+x | copies the clip's code and removes it |
| Copy | cmd+c | copies the clip's code |
| Paste at playhead | cmd+v | adds the copied clip to the script's `stack`, starting at the playhead |
| Duplicate | cmd+d | adds a copy right after the clip |
| Delete | delete | removes the clip from the script |

Every one of these rewrites the script and saves it, and cmd+z undoes it. When a change cannot be written safely, such as a line that holds more than the clip, or the last clip left, the script is left alone and the status bar says to edit the script instead.

## Playback

Space plays and pauses. The left and right arrows step one frame. The timeline bar has the same as buttons: |◀, ▶ and ▶|.

- The speed button beside them switches between 1x, 2x and 0.5x.
- Click the time at the right end of the bar to copy it in seconds, ready to paste into `at` or `trim`.
- `:set loop` plays in a loop. See [Commands](#commands).

## Voiceover

The red dot in the timeline bar records your voice from the playhead while the video plays. Space, or a click on the stop square, keeps the take. Esc discards it.

A kept take is saved as `voice/take-1.wav` beside the script, then `take-2.wav` and so on, and added to the script at the time you started:

```js
const voice_1 = sound("voice/take-1.wav")
// and voice_1.at(2.5) inside the script's stack
```

The microphone button at the left of the bar picks the input. A take of pure silence is not added: allow the microphone for Origami in System Settings, Privacy & Security. cmd+z removes a take from the script.

If the take's file can no longer be written while you record, for example because the disk is full, the recording stops by itself and the status bar says "recording stopped". What was recorded up to then is kept and added to the script like any other take; when nothing was recorded, nothing is added.

## Full-screen preview

The corner icon above the preview, `f`, or View › Full-screen Preview fills the window with the picture. Move the mouse to show a bar with play, the time and a line to drag the playhead along. It hides again after two seconds. Space and the arrows still work. Esc, `f` or the icon in the bar goes back.

## Keyboard mode

The left end of the status bar shows the mode. Origami starts in `-- MOUSE --`, where everything above works. Press esc with nothing else open, or click the mode, for `-- NORMAL --`: the timeline is driven with vim-style keys and the mouse no longer edits. `i` or another click goes back.

| Key | What it does |
|---|---|
| `h` `l` | one frame back, forward |
| `H` `L` | one second back, forward |
| `w` `b` | next, previous clip edge |
| `0` or `gg`, `$` or `G` | start, end |
| `j` `k` | pick which of the clips under the playhead is selected |
| `<` `>` | move the selected clip one frame earlier, later |
| `x` | delete the selected clip |
| space | play, pause |
| `m` + letter, `'` + letter | set a mark at the playhead, jump to it |
| `v` | start a range at the playhead. `v` again or esc drops it |
| `y` | copy the selected clip as code, such as `intro.trim("0:02.00", "0:05.00")`. With a range, only that part |
| `gd` | open the script at the clip's line in `$EDITOR`, or Zed |
| `/` | search clip names. Enter jumps to the next clip whose name holds the text, then `n` and `N` go to the next and previous |
| `za` `zM` `zR` | fold at the clip's line, fold all, unfold all |
| `t` | the [type tool](#type-tool) |
| `.` | run the last command again |
| `:` | type a [command](#commands) |

A number first repeats a move: `5l` is five frames forward, `3w` three clip edges.

## Commands

Press `:` in either mode, type a command and press enter.

| Command | What it does |
|---|---|
| `:1:05` or `:goto 1:05` | moves the playhead to that time |
| `:f 1800` | moves the playhead to that frame |
| `:e other.js` | opens another script in the same folder |
| `:set loop` | plays in a loop: between marks `a` and `b` when both are set, the whole video otherwise. `:set noloop` stops looping |
| `:export name.mp4` | exports at the timeline's size. See [Menus](#menus) |
| `:export --dry-run` | writes the ffmpeg command to `export.sh` instead of exporting |
| `:history`, `:history clear` | see [History](#history) |
| `:ir` | writes the timeline as JSON to `.origami/ir.json` in the project folder |
| `:help` | shows a sheet of keys. Esc closes it |
| `:q` | quits. With unsaved changes it says so, and `:q!` quits without saving |

## Projects

A project is a folder with a script in it, usually `edit.js`, and its media beside it.

Opening Origami with no script shows the launch screen: New project, Open…, Docs, and the projects you opened last. The line under the name is the version you are running. It turns blue when a newer one is out, and a click opens the download page.

- **New project** (cmd+n) asks for a name and a location, starting in `~/Movies/Origami`. Create makes a folder with that name, puts a starter `edit.js` in it and opens it. A folder that already has an `edit.js` is refused.
- **Open…** (cmd+o) opens any `.js` script. So does double-clicking a `.js` file in Finder (or Open With › Origami), File › Open Recent, and a path given when starting the app: `origami-app edit.js`.
- A script that cannot be read as text, or is over 16 MB, is not opened, and Origami says why.

## Saving

Click in the code to type. What you type is not saved, and the picture does not change, until cmd+s. Esc leaves the code. Everything done with the mouse saves by itself: sliders, timeline drags, the tools and the Motion tab.

- When a save cannot be written, such as onto a full disk or a read-only file, the status bar says why and the code stays on screen as unsaved.
- Opening another script, closing the window or quitting with unsaved changes asks first: Save, Discard or Cancel.
- You can edit the script in another editor while Origami is open. When the file changes on disk, Origami loads it and the picture follows. If you also have unsaved changes in Origami, a bar above the code asks: keep mine or use disk.
- Unsaved code survives a crash, a force quit and a power cut. The next time you open the script, it comes back with the same keep mine / use disk bar.

## History

Every save keeps a version of the script on your Mac, so you can see what changed and bring an earlier version back.

Open the list with the ↺ button at the right end of the code pane's header, cmd+y, File › History… or `:history`. It drops down above the code, newest first, grouped by day: Today, Yesterday, then the date. Each row shows when (just now, 12 min ago, then the time), what changed, and how many lines were added (green) and removed (red). The newest is marked current unless the code on screen has unsaved changes. A change to the numbers in one call on a named clip reads like `dome · temperature -4 → -10` or `camp · trim 12, 18 → 12, 20`; any other change shows its first changed line, a restore reads `↺ restored 11:31`, and text another program wrote into the script while you had unsaved changes reads `changed on disk`.

Click a version, or move with j and k (or the arrow keys) and press enter, to compare it with the code on screen, saved or not. Red lines are only in that version, green lines came after it with their line number, the rest is dimmed, and long unchanged stretches fold to `⋯ 12 unchanged lines`. `]c` and `[c` (or ↑ and ↓ in the bar) jump between changes, `← back` or esc returns to the list, and esc again closes it.

**Restore this version** (or r) puts that version back into the script and saves it as a new version on top; the list stays open. Nothing is lost: the code that was on screen is kept as a version first, even if it was not saved, and cmd+z undoes the restore. While the file has changed on disk, pick keep mine or use disk first.

- **When.** A version is kept on every save: cmd+s, Save when Origami asks about unsaved changes, and every edit that saves by itself, such as a slider, a timeline drag or the type tool. A save that changes nothing keeps nothing. Saves less than a minute apart that change the same lines become one version, so a whole slider drag is one entry showing where it started and where it ended. Choosing use disk after the file changed on disk keeps the code on screen as a version first.
- **Where.** In `~/Library/Application Support/Origami/history`, in a folder named from the script's path, readable only by your account. Never in the project folder, so versions do not travel with a shared project. The folder records the script's path; a folder that names another script, such as one copied in by hand, is left alone, and that script keeps no versions. Each version is flushed to disk in the background right after its save, so a power cut loses at most the versions still being written.
- **Removing.** A version keeps everything that was in the script when it was saved, including text you later delete, such as a pasted password, for up to a year, and the folder records the script's path. To remove a script's versions, click clear at the bottom of the list and then remove, or type `:history clear` and answer Remove; both ask first, because this cannot be undone. To remove them by hand, quit Origami and delete its folder in `~/Library/Application Support/Origami/history` (the `path` file in each folder names its script), or the whole `history` folder.
- **How long.** Everything from the last 7 days; then the last version of each hour up to 30 days; then the last version of each day up to a year. Older versions are removed, and each script keeps at most about 50 MB, dropping the oldest first. The thinning happens the first time a script is saved after Origami opens.

## Menus

Every command is also in the menu bar, with its shortcut.

| Menu | Items |
|---|---|
| Origami | About Origami, Check for Updates…, Share Usage and Crash Reports, Hide Origami (cmd+h), Hide Others (cmd+opt+h), Quit Origami (cmd+q) |
| File | New (cmd+n), Open… (cmd+o), Open Recent, Save (cmd+s), History… (cmd+y), Export… (cmd+shift+e), Close Window (cmd+w) |
| Edit | Undo (cmd+z), Redo (cmd+shift+z), Cut (cmd+x), Copy (cmd+c), Paste (cmd+v), Duplicate (cmd+d), Select All (cmd+a), Find (cmd+f) |
| View | Full-screen Preview (f), Bigger Text (cmd+=), Smaller Text (cmd+-), Actual Size (cmd+0), Fold (cmd+opt+[), Unfold (cmd+opt+]) |
| Window | Minimize (cmd+m), Zoom |
| Help | Origami Docs, What's New |

- Cut, Copy and Paste work on text while the caret is in the code, and on the selected clip otherwise. Duplicate works on the selected clip.
- About Origami shows the version. Help › Origami Docs opens these pages, and What's New opens the changelog.
- Check for Updates… says whether a newer version is out, with a Download button, or "Could not check for updates" when it cannot reach origami.video. Origami also checks when it opens. When the version you run is no longer supported, an alert says so each time it opens, with Download and Later; Origami keeps working either way. Starting it with `ORIGAMI_NO_UPDATE_CHECK=1` set skips the check.
- Share Usage and Crash Reports turns anonymous usage and crash reports on or off; a tick shows they are on. The launch screen asks once, Yes or No thanks, and nothing is sent before a yes. Starting Origami with `ORIGAMI_NO_TELEMETRY=1` set turns them off and hides the question and the menu item.

Export… (also ⋯ › export) opens the export dialog. It picks where to save, the resolution (the timeline's size, 4K, 1080p or 720p), the format (H.264, HEVC, or ProRes 422 saved as .mov), the quality (High, Standard or Small), the range (the whole timeline or the selected clip) and whether to include audio. ProRes has one quality. The dialog shows the size it expects the file to be. Enter exports, Esc cancels, and the choices are remembered for the script, except the range, which always opens on the whole timeline so that a clip selected later is never exported alone by surprise. They are kept in `~/Library/Application Support/Origami/export`, so they never travel with a shared project. The export is written to a hidden file beside the destination and moved into place when it finishes, so a failed or cancelled export leaves any file that was already there untouched, and the hidden file is removed even if Origami is force-quit or crashes mid-export. If the file already exists, Origami asks first: Replace writes over it, Keep Both saves the new export next to it as `name 2.mp4`, and Cancel goes back to the dialog. 4K draws text and shapes at full size, so names and captions stay sharp; video is scaled up from the clips. A size that would draw any clip over 16384 pixels on a side is refused with the clip's line; pick a smaller size or make the clip smaller. `:export name.mp4` still exports straight away at the timeline's size, asking the same question first when that file already exists, and `:export --dry-run` asks it before replacing an `export.sh`. The ffmpeg command in `export.sh`, like the one `origami render --dry-run` prints, reads the pictures of plain text and shapes from a folder named `origami-` and two numbers in the Mac's temporary folder (`$TMPDIR`), written when the command was made. Origami leaves that folder alone so the command still works later; delete it once you have run the command. A timeline with no such pictures leaves no folder.

- While the script has an error, Export… and `:export` first show the error and ask whether to export the last version that worked.
- One export runs at a time. Its progress shows in the status bar, and quitting while it runs asks first.
