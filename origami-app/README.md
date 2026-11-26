# origami-app

The Origami desktop app (GPUI). Edit the script in the app's code pane or in your own editor; the app reloads the file when it changes on disk.

```
cargo run --release -- path/to/edit.js
```

With no argument it opens the launch screen (New project, Open…, Docs). cmd+o opens a script from anywhere, and so does opening a .js file with Origami from Finder.

Starts in Mouse mode: drag the ruler to move the playhead, click a clip to select it, drag clips to move, drag clip edges to trim, scroll to zoom. Drags rewrite the script as you move.
Click a clip to select it and press delete to remove it. Click the script to type in it; cmd+s saves and applies, and closing with unsaved changes asks first. cmd+z undoes.
`Esc` switches to vim keys, `i` switches back. Type `:help` for keys.

Timeline toolbar: undo, redo, delete on the left; step and play centred under the video; playback rate (1x, 2x, 0.5x, preview only), a + menu (text, caption) and a ... menu (zoom, full screen, export) on the right.
cmd+shift+z redoes. f toggles full screen.
The launch screen lists recent projects from `~/Library/Application Support/Origami/recent`.
Drag the pane edges to resize the code pane and the timeline. cmd+= and cmd+- change the code font size.
The code pane supports selection: drag, shift+arrows, cmd+a, cmd+c, cmd+x, cmd+v, and cmd+arrows for line and document ends.
Code folds from the gutter arrows, cmd+opt+[ and cmd+opt+], or za/zM/zR in vim mode; long runs of definitions start folded. cmd+0 or ctrl+0 resets pane sizes, code size and timeline zoom.
Colour grading: see docs Color page. Autocomplete opens as you type. Click a call with number arguments for sliders, a colour wheel or a curve pad. cmd+f searches the code and jumps the timeline when the text is a clip name.
