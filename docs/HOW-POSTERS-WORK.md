# How a wallpaper gets its gallery poster

The picture you browse is a small, reusable representation of a wallpaper. An Engine scene can appear in the gallery without starting the scene. A video card can show a still frame without playing the whole video. Changing the desktop is a separate operation owned by Apply.

## Follow one poster

1. **Find the source.** The scanner builds a catalog from the configured folders and installed Workshop projects. It reads project metadata, the media type, paths and file stamps. An Engine project can supply its own preview image or video. A cached catalog helps the picker open before a fresh scan finishes.
2. **Decide what the view needs.** The gallery calculates which cards are visible, selected or needed for a transition. Their positions, shapes and identities also drive hit testing. The worker gets a bounded list of poster requests, so a large library can be browsed without putting every image in GPU memory.
3. **Get the pixels.** On a disk-cache hit the worker loads the prepared pixels. On a miss it decodes the image or extracts a video frame using Windows Media Foundation. It retains the aspect ratio and fits the picture into a standard 512 × 512 canvas. Matching source and stamp requests can share this work. A changed file gets a fresh cache identity.
4. **Choose the display size.** A distant card needs a 160 × 88 tile. A selected, hovered or detail card in Depth can use a 512 × 512 texture and progressively smaller copies down to 16 × 16. These copies, called mip levels, let the GPU sample a suitable resolution as a card changes size. Depth can display many neighbors with just one high-detail texture.
5. **Upload and draw.** Far tiles share one GPU texture called an atlas. High-detail posters occupy separate layers. On supported GPUs the app compresses far tiles with BC1 and near textures with BC7. Other GPUs can use RGBA pixels. The renderer uploads at most 8 MiB per frame and checks each slot's poster identity before drawing. Outgoing cards can retain their textures during a transition.

In Depth, when the reduced tiles are already available, the worker packs those tiles first and requests a frame. Neighboring cards and the selected card can appear at low detail while the worker finishes the selected image's high-detail conversion.

## What stays cached

| Where | What it holds | What it saves |
| --- | --- | --- |
| Disk | A derived catalog and separately keyed, reusable poster images | Rediscovering every item on launch and repeatedly decoding the same source |
| Worker memory | Decoded pixels, reduced tiles, and packed bytes waiting for upload | Repeating work during browsing and switching between quality levels |
| GPU memory | The far atlas and currently needed high-detail textures | Uploading the same visible poster on every frame |

A 512 × 512 four-channel canvas takes 1 MiB before compression. A 160 × 88 BC1 tile takes 7,040 bytes. The ordinary gallery supports up to 256 logical far slots and the high-detail array grows to at most 32 layers, subject to device limits. Slices, Wall and Geometric keep a recent card's small GPU tile until another card needs the slot. This lets a recently visited poster draw immediately when you reverse direction. The worker prioritizes first artwork and sharpens the current view once the first-artwork queue has been quiet for a short interval.

When the library is fresh and browsing is quiet, background work can prepare missing cache entries within available headroom. The default disk budget is 512 MiB with at least 10 GB of free space and 128 MiB below that threshold or when free space is unknown. A configured budget can override the default. Cache clearing invalidates older worker requests so an old job cannot put cleared data back. The original pictures, videos and Workshop projects stay in their source folders.

## Why a card can appear late or look empty

- A disk-cache hit saves decoding. Startup, texture conversion, upload and the next frame still take time. In the five-file Depth comparison, the warm portrait drew at 219 ms versus 338 ms previously. Cold high detail took 381 ms versus 357 ms previously. These timings describe one fixture and GPU.
- A corrupt image, a missing preview and a temporarily locked file have different outcomes. A transient file lock gets a bounded retry. An unusable Engine preview can get a deterministic placeholder. Failed video extraction can produce fallback pixels kept out of the permanent poster cache. A genuine decode failure can leave a gap.
- A texture becoming resident means it is ready on the GPU. A draw means a frame used its pixels. A GPU completion receipt confirms the submitted work finished. Measuring when that frame reached the monitor requires separate display timing. A screenshot alone cannot diagnose a hitch between two frames.

[Renderer and worker ownership](ARCHITECTURE.md) · [Trace guide](PROVIDER-POSTER-DIAGNOSTICS.md) · [Depth measurements](NEXT_SESSION_HANDOFF_HISTORY_2026-11-21_DEPTH.md)

For the complete path through both applications, including Skwd’s scanner, disk formats, GPU tiers and local source repositories, see the [poster pipeline source handoff](POSTER-PIPELINES-SOURCE-HANDOFF.md).
