# Effects JPEG decoder boundary

The user authorized one narrow direct dependency addition on 2026-10-14 after the installed image0.25.10/zune JPEG wrapper accepted truncated entropy with a valid EOI marker. Isolated probes also found jpeg-decoder0.3.2 and zenjpeg0.8.4 strict mode accepted the same four files. Neither rejected candidate is a project dependency.

Effects uses pinned `oxiarc-jpeg =0.4.2`, default features disabled (no arithmetic or Rayon), and its required oxiarc-core0.4.2 transitive package. Existing dependency versions remain unchanged. Upstream: https://github.com/cool-japan/oxiarc ; API: https://docs.rs/oxiarc-jpeg/0.4.2/oxiarc_jpeg/ . Both packages declare Apache-2.0. No decoder source is copied or patched. Publishing/packaging remains outside this task.

The shared immutable-byte reader/header guard admits single-frame JPEG8 baseline/progressive, one or three components; excludes CMYK/YCCK, arithmetic/lossless/hierarchical, MPO, invalid marker/scan/EOI structure and excess scans. The decoder uses tolerate_truncated=false, full scale, default YCbCr-to-RGB/fancy upsampling; grayscale replicates channels, alpha is255. Header dimensions and decoder dimensions/components/precision/color representation must agree.

Limits: input32MiB, edge4096,4194304pixels,100scans,3components,32MiB decoder output/internal16-bit plane budget,64MiB coefficient buffer. The application normalizes at most16MiB RGBA. The32MiB plane budget accommodates padding and valid maximum-pixel RGB4:4:4 inputs. These are per-buffer/process guards, not a64MiB total-RSS guarantee. Existing PNG validation and image dependency remain unchanged. Cooperative checkpoints surround nonpreemptible codec work.

Gallery still uses image::open(...).to_rgba8(); neither gallery nor Effects applies EXIF orientation or ICC color management. Effects preserves the encoded raster geometry and operates on encoded RGB values. The stricter decoder can round JPEG channels differently from the gallery decoder; whole decoder pixel parity is not claimed. PNG export strips JPEG metadata, preserves decoded dimensions and opaque alpha, and never applies wallpaper. Fixtures include an EXIF6 raw24x8 grayscale case and a Pillow/libjpeg32x32 color4:2:0 byte oracle.

The selected corruption regression requires shortened entropy with a valid EOI to fail before preview/publication. This does not provide a JPEG integrity checksum: a mutated stream that remains a valid JPEG cannot universally be detected as corrupted.
