# Changelog

What changed in each version of Origami. The launch screen shows the version you are running under the name; when a newer one is out, that line turns blue and underlined, and clicking it opens the download page.

## 0.1.0 (not yet released)

### Nov 19

- **Scripts run in a helper process.** A script that runs for more than 30 seconds, or uses more than 2 GB of memory, is stopped and its error shows like any other, even when one call never returns (a runaway regular expression, a huge array). Before, such a script kept running in the background and could take all of the Mac's memory and close Origami. While a script is still reading new media files it gets more time, up to 5 minutes.
- A script whose timeline is over 16 MB as text (tens of thousands of clips, or one huge value), or that animates more than 1,048,576 values, is stopped with an error. Before, reading such a timeline back could take several gigabytes of Origami's own memory.
- A captions file with more than 262,144 lines is refused with an error naming the line. Before, a file of many blank lines kept the script busy until it was stopped after 30 seconds.
- A force-quit or crash during an export no longer leaves a hidden half-written file beside the destination.
- A project can no longer run the Mac out of memory by naming one font or colour table through many spellings of its path (`./a.ttf`, `././a.ttf`, …): each file is read and kept once.
- An export at a size larger than the timeline that would draw a clip over 16384 pixels on a side, such as very large text on a small timeline exported at 4K, now stops with an error naming the clip's line instead of crashing Origami.
- Recovered unsaved code and remembered export choices are only offered for the exact script they were kept for, never for another script that happens to share their file name in Origami's folder. Export choices and unsaved code kept by earlier builds are not offered again.
- Saved versions written while quitting are flushed to disk before Origami exits.

### Nov 15

- **History.** Every save keeps a version of the script on your Mac. The ↺ button in the code pane, cmd+y, File › History… or `:history` lists them by day with what changed; click one to compare it with the code on screen and restore it as a new save, which cmd+z undoes. Versions stay in `~/Library/Application Support/Origami/history`, never in the project folder: everything from the last week, then one an hour for a month and one a day for a year, up to about 50 MB per script.
- **Export dialog.** ⇧⌘E opens a dialog to pick where to save, the resolution (timeline, 4K, 1080p, 720p), the format (H.264, HEVC or ProRes 422), the quality, the range (whole timeline or the selected clip) and whether to include audio. It shows the file size it expects, asks before writing over an existing file (Replace or Keep Both), and remembers your choices for each project. 4K keeps text and name tags sharp.
- **Usage and crash reports, only if you say yes.** The launch screen asks once whether to send anonymous usage and crash reports. Nothing is sent before a yes, and Origami › Share Usage and Crash Reports or `ORIGAMI_NO_TELEMETRY=1` turns it off. The Privacy section of the introduction lists exactly what is sent.
- **Type tool.** Click T beside the speed (or press `t`), then click the preview to add text at that spot and at the playhead.
- **Edit text on the picture.** Double-click a text in the preview to retype it there: Enter saves, Esc cancels, Shift+Enter starts a new line. The T tool now types straight onto the picture.
- **Font picker.** A font row in the click controls and the Motion tab opens a searchable list of the project's fonts and every font on this Mac. It works when the script names the font with a variable.
- **Clear history.** `clear` at the bottom of the History list, then `remove`, deletes every saved version of the script; `:history clear` does the same.
- A failed or cancelled export no longer damages a file that was already at the destination: Origami writes beside it and swaps the new file in only when the export finishes.
- The app now carries the licence texts of the open-source code it is built with (`THIRD-PARTY-LICENSES.txt`).
- Quitting from the Dock or logging out with unsaved changes saves them in `~/Library/Application Support/Origami/recovered` and offers them back the next time you open the script.
- If ffmpeg is missing, Origami says so and how to install it: `brew install ffmpeg`, then reopen Origami.
- The Open Recent list moved to `~/Library/Application Support/Origami`; your old list is carried over.
- **Safer to open other people's projects.** A project can no longer make Origami write outside its folder through a planted link, make ffmpeg open a web address, or preset a web address as the export destination. Devices, pipes and oversized scripts, caption, LUT and font files are refused instead of freezing the app. An export no longer writes through a link or stalls on a pipe planted at the destination, a remembered export size or quality the dialog does not offer is ignored, a project can no longer show its own "recovered" code in place of the script, and export settings are kept on your Mac instead of in the project folder. Crash reports carry only the source location and standard error messages. The update notice only links to Origami's own GitHub page.
- 4K, 1080p and 720p exports of a very wide or tall timeline stop at 16384 pixels on the long side instead of running the Mac out of memory.
- Quitting never hangs on saving history: when another window or program is writing the same script's history, Origami waits at most a quarter of a second.
- The crash log moves to `~/Library/Logs/Origami/origami.log.old` once it passes 4 MB, and Origami reads at most its newest megabyte when it opens.
- Colour grades preview and export faster: applying a grade to a frame takes about half the time, so grade sliders respond sooner.
- Exports with text, motion or grades finish about a fifth sooner.
- Undo history uses far less memory: each step keeps only the lines that changed.
- Typing in very long scripts is up to three times faster.
- Fixed a rare freeze where the preview, thumbnails or an export could wait forever on an ffmpeg that had already finished.
- **Menu bar.** Origami, File, Edit, View, Window and Help menus, with every existing shortcut on its item. New: ⌘N new file, ⌘O open, ⇧⌘E export, ⌘W close, ⌘M minimize, ⌘H hide.
- **Open….** A button on the launch screen and ⌘O open any script, and double-clicking a `.js` file in Finder (or Open With › Origami) opens it. Opening another script or quitting asks before losing unsaved changes, and quitting asks before stopping a running export.
- The launch screen shows the app's version and tells you when a newer one is available.
- Opening a project for the first time is about three times faster: media files are read in parallel.
- If the app crashes with unsaved changes, they are saved in `~/Library/Application Support/Origami/recovered` and offered back the next time you open the script.
- Works when opened from Finder or the Dock: ffmpeg is found in Homebrew's folders too.
- After jumping to a new spot, the full-quality picture arrives about 100 ms sooner.
- The app shows its first frame about three times sooner and uses about 400 MB less memory when a project has text.
- Colour-grade drags are up to a fifth faster, and graded playback does less work per frame.
- Dragging the edges of a named clip on the timeline works again.
- Letting go of the playhead after resting during a scrub shows the full-quality picture almost at once (about 30 ms instead of 260 ms).
- A cut or delete that brings another 4K clip under the playhead shows it about a third sooner.
- Moved, scaled or rotated layers draw up to a quarter faster.
- Every edit, drag and slider updates the picture sooner: the script runs in about half the time.
- Projects with many keyframes respond faster: every edit, drag, slider, undo and save on a project with long keyframed labels updates the picture in about half the time, and the cost no longer climbs with the number of keys.
- Big projects respond faster again: an edit, drag, slider, undo or save on a project with many keyframes updates the picture in about a third less time, and a script with hundreds of keyframed labels runs three to four times faster.
- Labels that follow another clip's position or opacity no longer slow the project down as the leader gains keyframes.
- On Macs with 8 GB of memory Origami keeps about 0.4 GB less in memory after scrubbing.
- Scrolling a very long script is smoother.
- **Exports about twice as fast**, using the Mac's built-in video encoder at the same picture quality. Files are larger, about twice the size of before. Frames wider or taller than 4096 pixels are saved as HEVC.
- Jumping far in 10-bit HEVC footage (such as DJI's) and opening a project are about 50 ms faster.
- The Motion tab draws more than twice as fast on big projects.
- Jumping onto a keyframe in footage without B-frames (such as DJI's) no longer decodes the whole second before it: the slowest far jumps are about a fifth faster.
- **Preview matches export on more kinds of footage:**
  - Jumping to a frame just before a keyframe in footage with B-frames shows that frame instead of a neighbour.
  - Variable-frame-rate footage (phone recordings) shows the same frame in the preview as in the export.
  - MPEG-TS footage (`.ts`, `.MTS`, `.M2TS`, `.vob`) shows the same frame in the preview as in the export, played or scrubbed.
  - A trimmed MPEG-TS or MPEG-PS clip (`.ts`, `.MTS`, `.vob`, `.mpg`) starts on the frame at the trim point, in the preview and in the export, instead of on the next keyframe. Its sound stays in step and it plays to the last frame of the file.
  - A clip that starts exactly half-way between two timeline frames no longer moves by a frame when an edit before it shifts its start by a rounding error.
- Fixed memory that kept growing while dragging colours, shapes or text, and during long exports with captions.
- **Fixed crashes and hangs:** clicking a clip right after deleting lines, a script that never finishes, very deep nesting, two exports at once, and clips placed off the frame or with a negative size.
- **Fixed script damage:** recording a take or pasting after a line ending in a comment or `;`, and dragging the edge of a clip whose `.trim(` spans several lines (now refused with a note).
- **Fixed wrong output:** very slow or very fast clips with sound, fades longer than the clip, empty text, rotated phone video, and accented letters in the default font.
- **Clearer errors:** most errors now point at the right line, corrupt or non-media files are reported instead of accepted, and export failures say what failed.
- `settings` called as `globalThis.settings(...)` or under another name reports its real line instead of "line 0".
- NaN, Infinity or text where a number belongs (`scale(NaN)`, `pos(Infinity, NaN)`, `wheel("shadows", { hue: "3" })`, curve points, a `rect` radius, a `line` width, `keys`, `delay`, `bezier`) is refused with a message that names the line and what takes the number.
- Names every JavaScript object carries (`valueOf`, `toString`, `constructor`, `__proto__`, …) are no longer taken for clip methods, eases, `each` modes or settings: `text("a").valueOf()` fails like any unknown method instead of making the clip vanish, and a clip used as text, as in `` text(`${title}`) ``, says "a clip cannot be used as text or a number" on its line. `video`, `sound`, `image`, `captions` and `text(…, { font })` called under another name, and Origami's internal `__` functions called from a script, report a plain message on the right line instead of a JSON position or "line 0"; an error quoting a very long name appears at once instead of after up to 30 seconds; and `settings({ fps: undefined })` keeps the frame rate instead of failing.
- **Fixed from the second round of testing:**
  - Changing preview quality from the ⋯ menu no longer crashes or breaks scripts. It edits your `settings(...)` call in place, keeping its layout.
  - A drag let go outside the window ends there.
  - The last frame of a clip holds instead of flashing black, and preview and export show the same frame at every time.
  - Splitting keeps the frame at the playhead and leaves every other frame as it was, including DJI footage at 29.97 fps on a 30 fps timeline, and is instant again.
  - Splits write times to the millisecond, with more decimals only when a frame needs them, and keep every frame in preview and export at any frame rate, including 24 fps.
  - Splitting a clip in `cat( )` no longer moves the clip after it by a frame, and splitting 24 fps footage on a 25 or 30 fps timeline no longer changes a frame beside the cut in the export. When the frame at the playhead cannot be cut exactly, the split lands on the nearest frame that can and a note says where.
  - A take or paste after a muted group no longer goes inside it.
  - Replacing a media file under the same name shows the new footage.
  - Autocomplete always fits in the window.
  - Times like 1:59.99 no longer show as 1:60.
  - The copied timecode lands on the frame shown.
  - In full-screen preview, keys no longer edit the hidden script.
- **Fixed from the third round of testing:**
  - Exporting onto a file your script uses is refused, however the name is spelled. Before, a different letter case or a symlink let it overwrite your footage.
  - A huge size or scale is an error on its line instead of crashing the app.
  - A take or paste no longer lands in the wrong group when a comment contains an apostrophe.
  - An `ffmpeg` that can't run, early on your PATH, no longer hides a working one.
  - Colours are hex only, such as `"#ffffff"`. Anything else is an error on its line instead of black. Click a colour in the script to pick it on the colour wheel.
  - Odd frame sizes are rounded up to even numbers, because video files need even sizes, and the status bar says so.
  - An export that stops making progress for 30 seconds is stopped and reported. Exports stop when the app quits. A second export waits for the first.
  - An export stops even when Origami is force-quit or runs out of memory, instead of running on in the background.

### Nov 14

- **Sharper preview.** The preview is drawn at the size your screen shows it, so small text looks the way it will in the export. `settings({ preview: "auto" | "full" | "half" | "quarter" })` or the ⋯ menu changes it.
- **Faster scrubbing on DJI footage.** While you drag the playhead, frames come from the camera's `.LRF` file, then the full frame replaces it when you let go. `settings({ proxy: false })` turns this off.
- **Frame-accurate preview.** Jumping, stepping and playing show the same frame, and it matches the export.
- **Voiceover.** The red dot in the timeline bar records from the playhead into `voice/take-N.wav` and adds the take to your script.
- **Full-screen preview.** The corner icon above the preview, or `f`, fills the window with the picture.
- **Clip menu.** Right-click a clip on the timeline to split, cut, copy, paste, duplicate or delete it, or use S, ⌘X, ⌘C, ⌘V, ⌘D and ⌫.
- Double-click empty timeline space to move the playhead there.
- Click the time in the timeline bar to copy it in seconds, ready to paste into a parameter.
- A muted sound no longer takes up a lane or stretches the timeline.
- The timeline scrolls and zooms in keyboard mode too.
- Autocomplete never covers the line you are typing.
- Dragging sliders, keys and clips is about twice as fast on large projects, and pasting long text is up to 200 times faster.
- Secondary colour correction takes `exposure`.

### Nov 13

- **Motion tab.** Every animated property of the selected clip as a row with its keys on a ruler, a Graph Editor for curves, and drag-to-connect between properties.
- **Graph tab.** A picture of which properties follow which.
- **Connections.** One clip's property can follow another's: `tag.pos(title.pos)`, with `add` and `mul` for offsets.
- **Shapes.** `rect` and `line`, text with a `background` plate and `align`, and colours with transparency.
- Drag a clip in the preview to place it; with keys, the drag sets the key at the playhead.
- Colour curves are edited by dragging points on the pad.

### Nov 12

- First version: the script language, preview, export, and the editor with timeline, colour tools, search, folding, autocomplete and hover help.
- Animated values with `keys(...)`, eases, typewriter text with `.shown` and per-letter animation with `.each`.
