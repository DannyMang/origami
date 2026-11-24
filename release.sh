#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
version=
dry=
for arg in "$@"; do
  case $arg in
    --dry-run) dry=1 ;;
    *) version=$arg ;;
  esac
done
fail() {
  echo "$1" >&2
  exit 1
}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "usage: ./release.sh <version, such as 0.2.0> [--dry-run]"
[ -z "${FFMPEG_DIR:-}" ] && ! grep -qsE '^[[:space:]]*(export[[:space:]]+)?FFMPEG_DIR=' .env || fail "FFMPEG_DIR is set: this release does not bundle ffmpeg"
tag=v$version
crates=(origami-lang origami-engine origami-app)
log=docs/pages/08-changelog.md
date=$(date '+%b %-d, %Y')
heading=$(grep -m1 '^## .* (not yet released)$' "$log" || true)

[ "$(git branch --show-current)" = main ] || fail "check out main first"
[ -z "$(git status --porcelain)" ] || fail "commit or discard your changes first"
! git rev-parse -q --verify "refs/tags/$tag" > /dev/null || fail "$tag already exists"
[ -n "$heading" ] || fail "$log has no \"(not yet released)\" heading"
for crate in "${crates[@]}"; do
  cargo fmt --check --manifest-path "$crate/Cargo.toml"
done

echo "version: $(sed -n 's/^version = "\(.*\)"$/\1/p' origami-app/Cargo.toml | head -1) -> $version"
echo "changelog: $heading -> ## $version ($date)"
if [ -f .env ]; then
  echo "reports: .env is present, so this build asks people about usage and crash reports"
else
  echo "reports: no .env, so this build never asks about usage and crash reports"
fi
if [ -n "${SIGN_ID:-}" ]; then
  echo "signing: Developer ID, notarised"
else
  echo "signing: ad-hoc, people approve it in Privacy & Security on first open"
fi

if [ -n "$dry" ]; then
  echo "dry run: would set the version in ${crates[*]/%//Cargo.toml} latest.json, retitle the changelog, test each crate and run ./bundle.sh"
  sum="<sha-256 of dist/Origami.dmg>"
else
  for crate in "${crates[@]}"; do
    sed -i '' "1,/^version = /s/^version = \".*\"$/version = \"$version\"/" "$crate/Cargo.toml"
  done
  sed -i '' "s/\"version\": \".*\"/\"version\": \"$version\"/" latest.json
  sed -i '' "s/^## .* (not yet released)$/## $version ($date)/" "$log"
  for crate in "${crates[@]}"; do
    cargo test --release --manifest-path "$crate/Cargo.toml"
  done
  ./bundle.sh
  sum=$(shasum -a 256 dist/Origami.dmg | cut -d' ' -f1)
  awk -v h="## $version ($date)" '$0 == h { on = 1; next } /^## / { on = 0 } on' "$log" > dist/notes.md
  [ -n "${SIGN_ID:-}" ] || printf '\nOrigami is not signed with an Apple Developer ID yet: the README says how to open it the first time.\n' >> dist/notes.md
  printf '\nSHA-256 of Origami.dmg: %s\n' "$sum" >> dist/notes.md
fi

cat <<EOF

dmg: $PWD/dist/Origami.dmg
sha-256: $sum

Check dist/notes.md and git diff, then run:

git commit -am "Release $version"
git tag -a $tag -m "Origami $version"
git push origin $tag
gh release create $tag dist/Origami.dmg --verify-tag --title "Origami $version" --notes-file dist/notes.md
git push origin main
(cd docs && bun run deploy)
EOF
