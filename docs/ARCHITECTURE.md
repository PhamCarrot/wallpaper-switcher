# Architecture

Wallpaper Switcher is a single Rust binary, `wallpaper-switcher-gpu-candidate`.
It is a tray-first desktop app: a GPU-rendered gallery picker, a coordinator
that talks to Windows and Wallpaper Engine, and background workers that
prepare posters and cache data.

```
Folders / Workshop projects
        │  scanner, catalog watch
        ▼
   Catalog ──────────────────────── catalog snapshot (disk)
        │
        │  poster worker (decode → 512² canvas → disk cache → GPU upload)
        ▼
   Gallery (iced + wgpu): Slices, Wall, Geometric, Depth
        │  user picks "Apply"
        ▼
   WallpaperCoordinator ── only component allowed to change the desktop
        │
        ├── Windows native wallpaper (pictures)
        └── Wallpaper Engine (local videos and Workshop projects)
        │
        ▼
   Transition cover: captures the outgoing frame, holds it over the
   desktop, then reveals the new wallpaper with the chosen effect
```

## Components

| Area | Where | What it does |
| --- | --- | --- |
| App entry & UI | `src/bin/`, `src/candidate_app/` | The gallery window, settings, detail view, playlists, theme designer and tray. Browsing never touches the desktop. |
| Catalog | `src/scanner.rs`, `src/library.rs`, `candidate_app/catalog*.rs` | Discovers local folders and Workshop projects, keeps a watch worker and a versioned startup snapshot. |
| Poster pipeline | `src/poster_worker*`, `src/gpu_gallery.rs` | Decodes sources to a 512² canvas, using Windows Media Foundation to extract video frames. Caches posters on disk and uploads tiles/high-detail textures within an 8 MiB per-frame budget. See [How posters work](HOW-POSTERS-WORK.md). |
| Coordinator | `src/coordinator.rs` | The single owner of desktop and provider state: rollback snapshots, capability checks, Apply, restore and shutdown. |
| Transitions | `src/transition.rs`, `src/d3d11_transition.rs`, `src/desktop_transition.rs`, `shaders/transition.hlsl` | 30 effects. Static images blend two captured frames; live wallpapers are revealed through a premultiplied cover because the live frame cannot be captured. |
| Provider glue | `src/engine_ipc.rs`, `src/engine_capture.rs`, `src/native_wallpaper.rs` | Wallpaper Engine control commands and state queries, frame capture, and Windows wallpaper APIs. |
| State | `src/config.rs`, `candidate_app/persistence.rs` | `config.toml` + v4 `candidate-state.toml`; loads stay read-only and older versions are migrated on save. |

## Safety rules

- Only `WallpaperCoordinator` mutates the desktop and providers. Gallery,
  search, filters and theme actions can never apply a wallpaper.
- Provider states stay distinct: a command being *accepted* is not the
  same as the provider *confirming* the change, and *unknown* is neither.
- `--preview` runs a fully isolated profile with Apply and global hooks
  disabled; it is classified before any side effect.
- A request that would hand a display from Wallpaper Engine to a native
  wallpaper is refused unless Engine absence can be confirmed — checked
  before any transition cover is shown.
- Cache publication is atomic: staged files, exact name sweeps, and
  guards that stop a cleared or superseded job from writing old data back.
