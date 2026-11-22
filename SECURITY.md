# Security

## Supported versions

Only the newest release gets fixes. Origami checks for a newer version each time it opens, and Origami › Check for Updates… checks on demand.

## Reporting a problem

Please do not open a public issue for a security problem. Use **Report a vulnerability** on the [Security tab](https://github.com/DannyMang/origami/security/advisories/new), which reaches the maintainer privately.

Include the Origami version shown on the launch screen, your macOS version, what someone could do with the problem, and the smallest script, media file or steps that show it. The fix is released first; the report is then made public, crediting you unless you would rather not be named.

## What counts

A script is code you run, so Origami keeps it inside limits. Any way around them is a security problem:

- A script can read a file your account can read only by naming it, and cannot reach the network, run programs or write files other than its own `origami.lock`.
- A script that runs too long or uses too much memory is stopped without taking Origami down with it.
- The update check downloads one small file from this repository and only ever offers a page of this repository to open.
- Usage and crash reports are sent only after a yes, and carry only what the README lists: never script contents, file names, paths or your user name.
- Origami never deletes, truncates or writes over footage, and writes a script only when it is saved.

Problems in ffmpeg itself belong with [FFmpeg](https://ffmpeg.org/security.html).
