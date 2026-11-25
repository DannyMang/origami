#!/usr/bin/env bash
set -euo pipefail

VERSION=8.1.2
SHA256=464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c
DAV1D=1.5.5
DAV1D_SHA256=ca96b1d0ba3f01f55de7231d775892b64994ba061aaeeaaa4f662a390b3abf9e
TARGET=15.0

cd "$(dirname "$0")"
out=ffmpeg
name=ffmpeg-$VERSION
dav1d=dav1d-$DAV1D
flags=(
  --arch=arm64
  --cc=clang
  --extra-cflags=-mmacosx-version-min=$TARGET
  --extra-ldflags=-mmacosx-version-min=$TARGET
  --extra-libs=-liconv
  --pkg-config=../dav1d/pkg-config
  --disable-autodetect
  --enable-videotoolbox
  --enable-audiotoolbox
  --enable-zlib
  --enable-bzlib
  --enable-iconv
  --enable-libdav1d
  --disable-network
  --disable-ffplay
  --disable-doc
  --disable-debug
)
cc=(
  clang
  -arch
  arm64
  -mmacosx-version-min=$TARGET
  -std=c99
  -O3
  -DNDEBUG
  -fvisibility=hidden
  -fomit-frame-pointer
  -ffast-math
  -fno-align-functions
)
lines="$name: ./configure ${flags[*]}
$dav1d: ${cc[*]}"

pinned() {
  [ "$(shasum -a 256 "$1" 2> /dev/null | cut -d' ' -f1)" = "$2" ]
}
fetch() {
  [ -f "$1" ] || {
    curl -fL --proto '=https' -o "$1.part" "$3"
    mv "$1.part" "$1"
  }
  pinned "$1" "$2" || {
    echo "$1 does not have the pinned SHA-256 $2: delete it and run again" >&2
    exit 1
  }
}

[ ! -x $out/ffmpeg ] || [ ! -x $out/ffprobe ] || [ "$(cat $out/configure.txt 2> /dev/null)" != "$lines" ] ||
  ! pinned $out/$name.tar.xz $SHA256 || ! pinned $out/$dav1d.tar.xz $DAV1D_SHA256 || exit 0

mkdir -p $out
fetch $out/$name.tar.xz $SHA256 "https://ffmpeg.org/releases/$name.tar.xz"
fetch $out/$dav1d.tar.xz $DAV1D_SHA256 "https://downloads.videolan.org/pub/videolan/dav1d/$DAV1D/$dav1d.tar.xz"

rm -rf $out/src $out/dav1d $out/ffmpeg $out/ffprobe $out/configure.txt
mkdir $out/src $out/dav1d
tar -xf $out/$name.tar.xz -C $out/src --strip-components 1
tar -xf $out/$dav1d.tar.xz -C $out/dav1d --strip-components 1
export ZERO_AR_DATE=1
(
  cd $out/dav1d
  plain=(
    cdf cpu ctx data decode dequant_tables getbits intra_edge itx_1d lf_mask lib log mem msac obu pal picture qm ref
    refmvs scan tables thread_task warpmv wedge arm/cpu
  )
  depth=(
    cdef_apply_tmpl cdef_tmpl fg_apply_tmpl filmgrain_tmpl ipred_prepare_tmpl ipred_tmpl itx_tmpl lf_apply_tmpl
    loopfilter_tmpl looprestoration_tmpl lr_apply_tmpl mc_tmpl recon_tmpl
  )
  asm=(
    itx lf_mask looprestoration_common msac refmvs cdef filmgrain ipred loopfilter looprestoration mc mc_dotprod
    cdef16 filmgrain16 ipred16 itx16 loopfilter16 looprestoration16 mc16 mc16_sve
  )
  cat > config.h << 'EOF'
#pragma once
#define ARCH_AARCH64 1
#define ARCH_ARM 0
#define ARCH_LOONGARCH 0
#define ARCH_LOONGARCH32 0
#define ARCH_LOONGARCH64 0
#define ARCH_PPC64LE 0
#define ARCH_RISCV 0
#define ARCH_RV32 0
#define ARCH_RV64 0
#define ARCH_X86 0
#define ARCH_X86_32 0
#define ARCH_X86_64 0
#define AS_ARCH_LEVEL armv8.6-a+crc
#define CONFIG_16BPC 1
#define CONFIG_8BPC 1
#define CONFIG_ASM_DEBUG 0
#define CONFIG_LOG 1
#define ENDIANNESS_BIG 0
#define HAVE_ALIGNED_ALLOC 1
#define HAVE_ASM 1
#define HAVE_AS_ARCHEXT_DOTPROD_DIRECTIVE 1
#define HAVE_AS_ARCHEXT_I8MM_DIRECTIVE 1
#define HAVE_AS_ARCHEXT_SVE2_DIRECTIVE 1
#define HAVE_AS_ARCHEXT_SVE_DIRECTIVE 1
#define HAVE_AS_ARCH_DIRECTIVE 1
#define HAVE_AS_FUNC 0
#define HAVE_C11_GENERIC 1
#define HAVE_CLOCK_GETTIME 1
#define HAVE_DLSYM 0
#define HAVE_DOTPROD 1
#define HAVE_ELF_AUX_INFO 0
#define HAVE_GETAUXVAL 0
#define HAVE_I8MM 1
#define HAVE_IO_H 0
#define HAVE_MEMALIGN 0
#define HAVE_POSIX_MEMALIGN 1
#define HAVE_PTHREAD_GETAFFINITY_NP 0
#define HAVE_PTHREAD_NP_H 0
#define HAVE_PTHREAD_SETAFFINITY_NP 0
#define HAVE_PTHREAD_SETNAME_NP 1
#define HAVE_PTHREAD_SET_NAME_NP 0
#define HAVE_SIGACTION 1
#define HAVE_SVE 1
#define HAVE_SVE2 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_UNISTD_H 1
#define PIC 3
#define PREFIX 1
#define TRIM_DSP_FUNCTIONS 1
EOF
  echo "#define DAV1D_VERSION \"$DAV1D\"" > vcs_version.h
  mkdir obj
  {
    for f in "${plain[@]}"; do echo "-c src/$f.c -o obj/${f/\//_}.o"; done
    for f in "${depth[@]}"; do
      for bits in 8 16; do echo "-DBITDEPTH=$bits -c src/$f.c -o obj/$f$bits.o"; done
    done
    for f in "${asm[@]}"; do echo "-c src/arm/64/$f.S -o obj/$f.S.o"; done
  } | xargs -P "$(sysctl -n hw.ncpu)" -L 1 "${cc[@]}" -I. -Iinclude -Iinclude/dav1d
  ar rcs libdav1d.a obj/*.o
  cat > pkg-config << 'EOF'
#!/bin/sh
case "$*" in
  --version | "--exists --print-errors dav1d >= "*) ;;
  "--cflags dav1d") echo -I../dav1d/include ;;
  "--libs dav1d") echo -L../dav1d -ldav1d ;;
  *) exit 1 ;;
esac
EOF
  chmod +x pkg-config
)
(
  cd $out/src
  ./configure "${flags[@]}"
  make -j "$(sysctl -n hw.ncpu)" ffmpeg ffprobe
  strip ffmpeg ffprobe
)
cp $out/src/ffmpeg $out/src/ffprobe $out/src/COPYING.LGPLv2.1 $out/
rm -rf $out/src $out/dav1d
echo "$lines" > $out/configure.txt
