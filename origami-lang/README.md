# origami-lang

The Origami edit language. A script is plain JavaScript; the last expression is the composition.

```
cargo run -- path/to/edit.js    # prints the timeline IR as JSON
cargo test
```

- `src/prelude.js`: clip library and flattening to a global timeline.
- `src/lib.rs`: Boa host, ffprobe, `origami.lock`, IR types (`Timeline`, `Clip`).

Language reference: `../docs/index.html`.

Errors a script author can fix are typed in `src/error.rs`. Today: `Missing`, a media path that does not exist; the app marks its line in red.
