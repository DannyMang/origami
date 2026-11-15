# Changelog

What changed in each version of Origami. The launch screen shows the version you are running under the name; when a newer one is out, that line turns blue and underlined, and clicking it opens the download page.

## 0.1.0 (not yet released)

### Nov 15

- **Menu bar.** Origami, File, Edit, View, Window and Help menus, with every existing shortcut on its item. New: ⌘N new file, ⌘O open, ⇧⌘E export, ⌘W close, ⌘M minimize, ⌘H hide.
- **Open….** A button on the launch screen and ⌘O open any script, and double-clicking a `.js` file in Finder (or Open With › Origami) opens it. Opening another script or quitting asks before losing unsaved changes, and quitting asks before stopping a running export.
- The launch screen shows the app's version and tells you when a newer one is available.
- Opening a project for the first time is about three times faster: media files are read in parallel.
- If the app crashes with unsaved changes, they are saved next to your script and offered back the next time you open it.
- Works when opened from Finder or the Dock: ffmpeg is found in Homebrew's folders too.
- After jumping to a new spot, the full-quality picture arrives about 100 ms sooner.
- The app shows its first frame about three times sooner and uses about 400 MB less memory when a project has text.
- Colour-grade drags are up to a fifth faster, and graded playback does less work per frame.
- Dragging the edges of a named clip on the timeline works again.
- Letting go of the playhead after resting during a scrub shows the full-quality picture almost at once (about 30 ms instead of 260 ms).
- A cut or delete that brings another 4K clip under the playhead shows it about a third sooner.
- Moved, scaled or rotated layers draw up to a quarter faster.
- Every edit, drag and slider updates the picture sooner: the script runs in about half the time.
- **Exports about twice as fast**, using the Mac's built-in video encoder at the same picture quality. Files are larger, about twice the size of before. Frames wider or taller than 4096 pixels are saved as HEVC.
- Jumping far in 10-bit HEVC footage (such as DJI's) and opening a project are about 50 ms faster.
- The Motion tab draws more than twice as fast on big projects.
- Jumping onto a keyframe in footage without B-frames (such as DJI's) no longer decodes the whole second before it: the slowest far jumps are about a fifth faster.
- Fixed memory that kept growing while dragging colours, shapes or text, and during long exports with captions.
- **Fixed crashes and hangs:** clicking a clip right after deleting lines, a script that never finishes, very deep nesting, two exports at once, and clips placed off the frame or with a negative size.
- **Fixed script damage:** recording a take or pasting after a line ending in a comment or `;`, and dragging the edge of a clip whose `.trim(` spans several lines (now refused with a note).
- **Fixed wrong output:** very slow or very fast clips with sound, fades longer than the clip, empty text, rotated phone video, and accented letters in the default font.
- **Clearer errors:** most errors now point at the right line, corrupt or non-media files are reported instead of accepted, and export failures say what failed.
- **Fixed from the second round of testing:**
  - Changing preview quality from the ⋯ menu no longer crashes or breaks scripts. It edits your `settings(...)` call in place, keeping its layout.
  - A drag let go outside the window ends there.
  - The last frame of a clip holds instead of flashing black, and preview and export show the same frame at every time.
  - Splitting keeps the frame at the playhead and leaves every other frame as it was, including DJI footage at 29.97 fps on a 30 fps timeline, and is instant again.
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
