# How a wallpaper gets its gallery poster

Each card in the gallery shows a small, reusable picture of a wallpaper.
An Engine project or a video can appear in the gallery without running —
a still frame is enough. Changing the actual desktop is a separate
operation owned by Apply.

## Follow one poster

1. **Find the source.** The scanner builds a catalog from the configured
   folders and installed Workshop projects: type, paths, stamps and
   preview metadata. A saved catalog lets the picker open before a fresh
   scan finishes.
2. **Decide what the view needs.** The gallery works out which cards are
   visible or selected and hands the worker a bounded request list, so a
   large library never has to fit in GPU memory at once.
3. **Get the pixels.** On a disk-cache hit the worker loads prepared
   pixels. On a miss it decodes the image or extracts a video frame with
   Windows Media Foundation, keeps the aspect ratio and fits it onto a
   standard 512 × 512 canvas.
4. **Pick a display size.** Distant cards need a small tile; selected or
   detail cards can use the full 512 × 512 plus progressively smaller
   copies (mip levels) so the GPU always has a suitable resolution.
5. **Upload and draw.** Small tiles share one large GPU texture (an
   atlas); high-detail posters get their own layers. On supported GPUs
   tiles are compressed (BC1/BC7); other GPUs use raw RGBA. Uploads are
   capped at 8 MiB per frame so browsing stays smooth.

## What stays cached

| Where | What it holds |
| --- | --- |
| Disk | A derived catalog and keyed, reusable poster images |
| Worker memory | Decoded pixels and packed tiles waiting for upload |
| GPU memory | The tile atlas and the high-detail textures currently needed |

The disk cache sizes itself to roughly one poster per library item
(never more than a quarter of the free disk space, with a 128–512 MiB
floor; a `cache_max_mib` setting overrides it). The original files stay
in their source folders — nothing is moved or modified.

## Why a card can appear late or look empty

- A cache hit skips decoding but conversion, upload and the next frame
  still take a moment.
- A corrupt image, a missing preview and a temporarily locked file are
  different cases: the app retries locked files, uses a placeholder for
  unusable Engine previews, and leaves a gap only for a genuine decode
  failure.

See [Architecture](ARCHITECTURE.md) for who owns each stage.
