#!/usr/bin/env bash
set -euo pipefail

VERSION=8.1.2
SHA256=464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c
TARGET=15.0

cd "$(dirname "$0")"
out=ffmpeg
name=ffmpeg-$VERSION
tar=$out/$name.tar.xz
flags=(
  --arch=arm64
  --cc=clang
  --extra-cflags=-mmacosx-version-min=$TARGET
  --extra-ldflags=-mmacosx-version-min=$TARGET
  --extra-libs=-liconv
  --pkg-config=/usr/bin/false
  --disable-autodetect
  --enable-videotoolbox
  --enable-audiotoolbox
  --enable-zlib
  --enable-bzlib
  --enable-iconv
  --disable-network
  --disable-ffplay
  --disable-doc
  --disable-debug
)
line="$name: ./configure ${flags[*]}"

[ ! -x $out/ffmpeg ] || [ ! -x $out/ffprobe ] || [ "$(cat $out/configure.txt 2> /dev/null)" != "$line" ] || exit 0

mkdir -p $out
[ -f "$tar" ] || {
  curl -fL --proto '=https' -o "$tar.part" "https://ffmpeg.org/releases/$name.tar.xz"
  mv "$tar.part" "$tar"
}
[ "$(shasum -a 256 "$tar" | cut -d' ' -f1)" = $SHA256 ] || {
  echo "$tar does not have the pinned SHA-256 $SHA256: delete it and run again" >&2
  exit 1
}

rm -rf $out/src $out/ffmpeg $out/ffprobe $out/configure.txt
mkdir $out/src
tar -xf "$tar" -C $out/src --strip-components 1
(
  cd $out/src
  export ZERO_AR_DATE=1
  ./configure "${flags[@]}"
  make -j "$(sysctl -n hw.ncpu)" ffmpeg ffprobe
  strip ffmpeg ffprobe
)
cp $out/src/ffmpeg $out/src/ffprobe $out/src/COPYING.LGPLv2.1 $out/
rm -rf $out/src
echo "$line" > $out/configure.txt
