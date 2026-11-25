# Supported formats

Origami reads video and sound with the ffmpeg inside the app, which includes the dav1d AV1 decoder, and HEIC, AVIF, JPEG XL and WebP pictures with macOS itself. Everything listed here was tried on an M3 Mac with macOS 15.6.

## Video

These work in `video`, in the preview and in exports:

- H.264
- HEVC: 8-bit, 10-bit and HLG HDR phone footage. HDR is shown as it is stored, not adjusted for a normal screen.
- AV1: 8-bit, 10-bit and 4:4:4
- ProRes 422 and 4444
- DNxHR, GoPro CineForm and DV
- VP8 and VP9, also 10-bit
- MPEG-2, MPEG-4, Motion JPEG and WMV
- GIF and animated PNG

in MP4, MOV, MKV, WebM, MTS, TS, MPG, AVI, FLV and WMV files.

Transparency stored in HEVC or VP9 video is dropped: the clip shows solid.

AV1 video is decoded by the Mac's own AV1 hardware where it takes the film, which is 8-bit and 10-bit 4:2:0 on an M3 or later chip, and by dav1d everywhere else: on M1 and M2 Macs and for 4:4:4 films. The M1 and M2 path has not been run on those Macs.

## Pictures

These work in `image`, in the preview and in exports:

- PNG, TIFF, BMP, TGA, OpenEXR, PSD and JPEG 2000
- JPEG. A photo saved on its side with a rotation tag is shown upright.
- HEIC and HEIF, including the photos iPhones take
- AVIF: 8-bit, 10-bit and with transparency
- JPEG XL
- WebP. An animated WebP shows its first frame.

HEIC, AVIF, JPEG XL and WebP pictures, and JPEGs with a rotation tag, are read by macOS. Their colours are converted to sRGB, so a wide-colour (Display P3) or HDR photo looks as it does in Preview on a normal screen.

SVG pictures are not supported: export them as PNG first. An animated GIF goes in `video`, not `image`. Camera RAW files have not been tried.

## Sound

These work in `sound`, and as the sound of a video: MP3, AAC, ALAC, FLAC, WAV, AIFF, AC-3, E-AC-3, Opus, Vorbis and WMA.

## When a file cannot be read

The first time a script uses a file, Origami decodes one frame of it. A file that fails gives an error on its line that names the format and what to do, such as:

```
line 3: image("logo.svg"): SVG pictures are not supported: export it as PNG first
line 2: video("clip.mkv"): this is unrecognised video, which Origami cannot read: convert it to H.264, HEVC or ProRes first
line 5: video("take.mp4"): this H264 video cannot be read: the file may be damaged
```

The check runs once for each file and again when the file changes. It reads the first frame only, so a file damaged further in still shows blank from that point.

## Exports

Exports are H.264, HEVC or ProRes video with AAC sound.
