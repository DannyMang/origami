# Changelog

What changed in each version of Origami. The launch screen shows the version you are running under the name; when a newer one is out, that line turns blue and underlined, and clicking it opens the download page.

## 0.1.0 (not yet released)

### Nov 22

- A timeline that shows more videos at once than Origami kept decoders for (four on a Mac with 8 GB of memory, six on larger ones), such as a wall of 12 picture-in-picture videos, now keeps one decoder for each video on screen instead of starting ffmpeg again for every layer of every frame. A 4-second 1080p export with 12 videos, 14 titles and 14 moving shapes used about 179 seconds of processor time and now about 17. The pictures are unchanged.
- In footage with B-frames, which most cameras and phones record, Origami looks up where each clip's first frame is with a short ffmpeg run. An export now does this for every clip at once, on all cores, before it draws the first frame, and the preview does it in the background after each change, instead of one clip at a time as playback reaches it. Playing across a cut no longer pauses for it, and a montage of 300 cuts of 0.2 seconds waited about 30 seconds in total for these lookups during its export and now about 4 seconds at its start (measured while the Mac was busy with other work). The pictures are unchanged.
- While playing forwards and exporting, each video's next frame is now read from ffmpeg while the current frame is drawn, instead of only when it is needed. Measured while the Mac was otherwise fairly quiet, a 4-second 1080p export with 12 cuts, a picture-in-picture video, roughened images and a matte finishes about 15 to 20 percent sooner, and one with 13 videos on screen about 15 percent sooner; a montage of 300 short 640×360 cuts is unchanged. Each video being played holds one frame more, about 20 MB more at most in these exports. The pictures are unchanged.
- When Origami is force-quit or crashes during an export, the folder of temporary layer pictures that export was using is now removed along with the half-written file, instead of staying in the Mac's temporary folder. An export of 20 titles and 20 boxes over 720p video left 8.5 MB behind each time.
- A colour table (`.cube`) file used by `lut` or `look` is kept in memory once per version of the file only for the 32 most recently read, instead of every version and every file read since Origami opened. Each version of a 65-point table takes 3.3 MB, so re-exporting one from a grading app 50 times while Origami was open kept about 160 MB of old tables.
- Origami keeps at most the 16 fonts used last read in memory, instead of every font any title used since it opened. A font can take a lot of memory once read (Arial Unicode, which draws letters a title's own font lacks, such as Chinese or Japanese, about 360 MB; most fonts 1 to 15 MB), and playing 120 titles each in a different font held about 1.36 GB until Origami quit; now it stays under about 520 MB and falls back as titles leave.
- The timeline's clip thumbnails no longer pile up for as long as Origami is open. Every thumbnail ever shown, at every zoom level and trim, stayed in memory twice (about 70 KB in the app and 70 KB of graphics memory each, so 3,000 of them held about 400 MB). Now thumbnails not shown since the timeline last gathered 500 are let go, from both; a thumbnail on screen stays, and a sound's waveform is kept.
- Opening a script whose media Origami has not read before (no `origami.lock` yet, or new or changed files) reads each file once. Before, every clip after the first that used the same file read it again, one at a time: a montage of 300 cuts from 5 new files took about 14 seconds to open and now about 0.2 seconds, and 300 files each used twice took about 14 seconds and now about 3. The timeline and `origami.lock` are unchanged.
- A long script with more than about 10,000 names defined at its top level, as generated scripts can have, now evaluates instead of stopping with `RuntimeLimitError: reached the maximum stack size on this execution`, which named no line. A 50,000-line script with 10,000 named clips evaluates in about 4.6 seconds. Scripts that make their clips in a loop or an array, rather than naming each one, evaluate faster: 12,000 titles in about 1.4 seconds.

### Nov 21

- Picture effects (`roughen`, `blur`, `lens`, `displace`, `noise`, `fractal` and `levels`) on an image, shape or text, and on a video stepped to a lower rate with `fps`, are worked out once and kept while the picture and the effect's settings stay the same, instead of again on every frame. A 1080p export of 2 seconds with a roughened 1400×900 image in multiply over video took about 2.2 s and now takes about 0.7 s, and its memory peaked at about 58 MB and now at about 44 MB, because an image or shape whose effect never changes keeps only the finished picture. Playing such a timeline saves about 30 ms a frame for each roughened full-size image. A playing video's effect is still worked out for each new frame, so it holds no extra memory. The pictures are unchanged.
- `blur` on a picture that changes every frame, such as a video, is worked out on every core and in place: 2 seconds of blurred 1080p video export in about 1.8 s instead of 3.1 s, and a blurred 1080p frame takes about 30 MB less memory. The pictures are unchanged.
- An effect on a clip drawn less than a pixel wide, such as a video scaled down to almost nothing, no longer stops Origami; the clip is simply not drawn, as without the effect.
- `roughen` measures how far each pixel is from the picture's edge about four times faster. A roughened 1400×900 image is ready about 15 ms sooner the first time it shows, and 2 seconds of a roughened 1344×756 video over another export in about 1.3 s instead of 1.5 s. The pictures are unchanged.
- A `path` whose points move only for part of its clip no longer works out its outline again for every later frame on every frame drawn. A 60-second 320×180 export of a 16-point curved path animated over its first 2 seconds took about 5.5 s and now takes about 1.1 s. The pictures are unchanged.
- Groups, mattes and layers that are blended in multiply, moved, turned or stepped are drawn with less work: the clear pixels of a layer are skipped, a layer that sits on whole pixels reads one pixel of its picture instead of four, and a matte's mix and a group's colour pass use every core. A 2-second 1080p export of a group over video took about 0.95 s and now about 0.7 s, and of a video behind an alpha matte about 1.6 s and now about 1.2 s; an image in multiply over video uses about a quarter less processor time. With the changes above, a 4-second 1080p export with 12 cuts, a picture-in-picture video, three roughened images in multiply, two roughened moving shapes, a group and a matte took about 11.4 s and now takes about 4.5 s, using less than half the processor time, at the same peak memory. The pictures are unchanged.

### Nov 20

- With the search bar open, clicking in the code now sends your typing to the code. Before, it kept going into the search box. Click the bar or press ⌘F to type in the search again.
- Origami is now open source under the GNU General Public License, version 3: anyone may use, change and share it, and whoever shares it or a changed version shares the source under the same licence. Before, it was under the Functional Source License, which did not allow competing commercial use for two years.
- Origami's own text, in the code, the timeline and every panel, is now set in Paper Mono, which is built into the app. These docs use it too. Text in your videos is unchanged.
- While scrubbing, a `.LRF` proxy beside a clip is only used when it is a plain file. One that is something else, such as a named pipe, is skipped in favour of the clip itself; before, it could stall the preview for 20 seconds at a time.
- Reading media takes far less memory for huge pictures: a 1 MB, 16000×16000 PNG made each read of it take about 1 GB, and now takes about 6 MB, because ffprobe no longer decodes the picture just to learn its size. A project opened for the first time reads at most 8 new media files at once instead of 32. The sizes, lengths and kinds Origami learns from each file are unchanged.
- Playlist files are refused: `video`, `sound` or `image` on an HLS playlist (a `.m3u8` or `.m3u` file, or any file starting with `#EXTM3U`) or an ffmpeg concat list (a file starting with `ffconcat`) now gives the error "playlist files are not supported: use a video or sound file" on its line at once, even when `origami.lock` lists the file. The preview, thumbnails, waveforms and exports never hand such a file to ffmpeg either, and a `.LRF` proxy that is a playlist is skipped in favour of the clip itself. Before, a playlist that keeps waiting for its next part could stall them for 20 seconds or more each time.
- The timeline's thumbnails fill in about twice as fast for footage that has a small `.LRF` copy beside it, as DJI and GoPro cameras record: they are read from that copy, so they look the same at the same size. A copy that cannot be read, or that ends before the clip does, is skipped for the clip itself.
- Pressing play no longer freezes the window while Origami reads the media it is about to play. The play button changes at once and the playhead waits where it is until the sound is ready, so a file that takes long to read (up to 20 seconds for one that never answers) no longer stops typing, scrolling or pausing meanwhile.
- After a click in the code, backspace always removes the letter behind the caret. Before, the first backspace left a hidden one-letter selection, so the second one removed the letter in front; a pointer that slipped a pixel during the click did the same on the first. Dragging more than 3 pixels still selects.
- The caret in the search field sits right after the last letter instead of a gap away, and in front of the greyed "Search…" hint while the field is empty.
- A media file that never ends, such as a live HLS playlist (`.m3u8`) that keeps waiting for its next part, no longer freezes the preview, an export or the timeline's thumbnails until Origami restarts. An ffmpeg or ffprobe that sends nothing for 20 seconds is now stopped: the clip shows black where it could not be read, and a file the script is still reading gives an error on its line.
- A sound's waveform in the timeline covers at most the first day of the file, and is worked out without keeping the whole decoded sound in memory. Before, a tiny sound file with one sample a second could take gigabytes of Origami's memory.
- Exports use less memory: a 1080p export keeps only the video frames its layers need, so its peak memory fell from about 105 MB to about 65 MB (about 88 to 52 MB with two or three video layers). A 4K export with three or more overlapping video layers, one of them slowed down, no longer restarts a decoder for every repeated frame, so it finishes several times sooner (about 6 s instead of 35 to 60 s for a 4-second example).
- Stepping backwards one frame at a time no longer pauses about every second: while you step through the frames Origami has kept, it reads the ones before them in the background. Stepping back toward a cut also reads the end of the clip before it, so crossing the cut no longer waits about half a second. Stepping back 120 frames of 4K footage across two cuts took about 3.6 s with two to three pauses of about 0.3 s, and now takes about 1.3 s. Stepping back three frames at a time pauses about a third as long. Going back over the same few seconds again can find a few more frames to read, because the frames read ahead take up to a quarter of the preview's frame memory.
- A long timeline cut from a few media files uses less processor time while Origami sits idle: it checks each file for changes once per screen refresh instead of once per clip (with 451 clips from 34 files, about 0.05 ms of every 16 ms instead of 0.4 ms).

### Nov 19

- A timeline with more lanes than fit, such as 55 stacked layers, keeps every lane at least half its usual height instead of squeezing them into a few pixels each. Scrolling up or down then moves through the lanes, and cmd+scroll zooms; when every lane fits, scrolling zooms as before.
- **Scripts run in a helper process.** A script that runs for more than 30 seconds, or uses more than 2 GB of memory, is stopped and its error shows like any other, even when one call never returns (a runaway regular expression, a huge array). Before, such a script kept running in the background and could take all of the Mac's memory and close Origami. While a script is still reading new media files it gets more time, up to 5 minutes.
- A script whose timeline is over 16 MB as text (tens of thousands of clips, or one huge value), or that animates more than 1,048,576 values, is stopped with an error. Before, reading such a timeline back could take several gigabytes of Origami's own memory.
- A captions file with more than 262,144 lines is refused with an error naming the line. Before, a file of many blank lines kept the script busy until it was stopped after 30 seconds.
- Long captions files load several times faster: a day-long .srt with 30,000 captions took about 20 seconds on every run, and now takes about 6.
- Reading back a script's timeline takes far less of Origami's memory: a timeline with millions of animated numbers and one bad clip took about 900 MB, and now takes under 50 MB. The bad clip's error now always names its own line, even when a clip before it changes over time.
- If Origami is updated or moved while it is open, a script that needs a fresh helper now says "Origami was updated: restart it to run scripts" instead of running with the new version's helper.
- The update notice only shows a version made of numbers and dots, and only links to a plain page of Origami's own GitHub project.
- A force-quit or crash during an export no longer leaves a hidden half-written file beside the destination.
- A project can no longer run the Mac out of memory by naming one font or colour table through many spellings of its path (`./a.ttf`, `././a.ttf`, …): each file is read and kept once.
- An export at a size larger than the timeline that would draw a clip over 16384 pixels on a side, such as very large text on a small timeline exported at 4K, now stops with an error naming the clip's line instead of crashing Origami.
- Recovered unsaved code and remembered export choices are only offered for the exact script they were kept for, never for another script that happens to share their file name in Origami's folder. Export choices and unsaved code kept by earlier builds are not offered again.
- Saved versions written while quitting are flushed to disk before Origami exits.
- The code pane shows at most 500 warnings at a time, plus the script's error. A line of tens of thousands of brackets no longer freezes Origami for seconds on opening and after every keystroke, and the Graph tab no longer freezes on a script of about a thousand clips that follow one another. Autocomplete, search and folding also stay quick on scripts with tens of thousands of names, matches or brackets.
- A line of hundreds of thousands of characters no longer slows the code pane or fills the Mac's memory: its error squiggle, search highlights and colours are only drawn for the part on screen. Scrolled more than about 4,000 characters into such a line, colouring starts 4,096 characters before the visible part, so a string that began further left can show in code colours.
- The Graph tab, the follow menu and "go to source" stay quick on scripts of many thousands of clips: with 20,000 clips each took over 2 seconds, and now takes a fraction of a second.
- The first picture after opening a script with DJI footage shows about a quarter sooner (about 0.12 s instead of 0.16 s): Origami now reads the full-size file and its `.LRF` at the same time, and reads the `.LRF` twice as fast.
- Scrubbing DJI footage is smoother: while dragging slowly each picture takes about a sixth less time, and a fast drag that jumps more than 20 frames now starts reading the `.LRF` at the new spot instead of reading through every frame in between, so the picture updates about a quarter more often.
- Zooming a clip without turning it uses about a fifth less processor time per frame, and such exports finish a little sooner.
- Stepping backwards through 4K footage pauses about half as long (about 0.23 s instead of 0.45 s) each time Origami has to read the frames before the ones it has kept: it now starts reading them while you are still stepping through the kept ones.
- When macOS stops one of the ffmpeg processes the preview reads video from, for example because memory is short, the preview starts a new one straight away instead of showing that clip black for up to a second.
- A media file Origami could not read once, for example while it was still being copied or while the Mac was short of memory, is read again after a few seconds of playback instead of staying misread until it changes or Origami restarts.

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
- Colour backgrounds, see-through overlays, pictures with transparency and fades draw about two and a half times faster in the preview and in exports: blending now uses every core.
- Big shapes and vignettes appear sooner: a large rounded rectangle or a vignette is drawn in under half the time, so dragging their sliders responds faster.
- Playing graded footage takes about a tenth less time per frame and holds about 100 MB less memory, because the preview reuses its frame memory instead of asking for new memory every frame. Exports finish 5 to 15% sooner for the same reason.
- Crossing a cut while playing or scrubbing no longer waits about 10 ms for the previous clip's decoder to close.
- The first press of play after opening a project no longer freezes the window while Origami reads each media file one after another (about 0.2 s on a 10-file project): the files are now read in parallel in the background as soon as the script loads, and exports start about 0.15 s sooner.
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
