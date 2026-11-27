# origami-lang

The Origami edit language. A script is plain JavaScript; the last expression is the composition.

```
cargo run --manifest-path ../origami-engine/Cargo.toml -- ir path/to/edit.js    # prints the timeline IR as JSON
cargo test
```

- `src/prelude.js`: clip library and flattening to a global timeline.
- `src/lib.rs`: Boa host, ffprobe, `origami.lock`.
- `src/ir.rs`: IR types (`Timeline`, `Clip`).

Language reference: https://origami.video/docs/.

Errors a script author can fix are typed in `src/error.rs`. Today: `Missing`, a media path that does not exist; the app marks its line in red.
