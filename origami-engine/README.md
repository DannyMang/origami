# origami-engine

Preview frames, audio playback and ffmpeg export for an Origami timeline. Needs `ffmpeg` and `ffprobe` on PATH.

```
cargo run --release -- render edit.js out.mp4 [--dry-run] [--relock]
cargo run --release -- ir edit.js
```

- `export.rs`: timeline to one ffmpeg command.
- `preview.rs`: frame at time t, composited on the CPU from streaming ffmpeg decoders.
- `audio.rs`: timeline audio through cpal; its sample count is the playback clock.
- `text.rs`: text rasterizer shared by preview and export.
