# Wallpaper Switcher ownership and verification

The Windows application has one Rust package and one GPU binary, `wallpaper-switcher-gpu-candidate`. `src/bin/gpu_candidate.rs` enters the private `candidate_app` module. Cargo.toml and Cargo.lock define current dependency versions.

## Gallery and worker

For the user-facing explanation of source discovery, disk caching, GPU memory and visible frames, see [How posters work](HOW-POSTERS-WORK.md).

| Owner | Responsibility |
| --- | --- |
| `src/candidate_app/gallery.rs`, `gallery_presentation.rs`, `hex_presentation.rs`, `wall_presentation.rs` inside `src/candidate_app/` | Current layout, motion and selection, plus the frame used for drawing, hits, and poster demand. |
| `src/candidate_app/motion_mailbox.rs` | One latest camera payload and one MotionReady marker; generation-safe drain into the existing animation reducer. |
| `src/candidate_app/catalog_watch.rs` | One catalog scan/watch worker; native notification ownership outside the state mutex, generation-checked publication and bounded owner shutdown. |
| `src/candidate_app/catalog_snapshot.rs`, `catalog_watch.rs`, `runtime.rs` | Bounded/versioned derived startup catalog, read-only load and stale display; reducer-accepted worker persistence through existing staged publication. Freshness guards startup and automatic provider authority separately from content revision. |
| `src/scanner.rs`, `src/library.rs`, `src/candidate_app/catalog.rs` | Cached Steam discovery, bounded ordered metadata/header phases and single-pass precise stamps; accepted content revision/diff and stable healthy poster remap. Scan request cancellation uses its own clock. |
| `src/candidate_app/ordinary_roll.rs` | Outgoing poster and material pins through gallery transitions, with stable identity checks and shared layer demand. |
| `src/library/media_still.rs`, `src/candidate_app/images.rs`, `src/poster_worker.rs` | Capture source identity and freshness, start the worker, submit bounded poster work, and signal completion. |
| `src/poster_worker/cache.rs`, `src/poster_worker/colour.rs`, `src/file_publication.rs` | Cache root and epoch guards, colour analysis, and atomic staged publication. |
| `src/candidate_app/events.rs` | Reducer installs current catalog and poster results, including `Wake::PostersReady`. |
| `src/gpu_gallery.rs` | Atlas allocation and uploads, texture residency, and rendering. |

Catalog discovery preserves root/enumeration order before scoped I/O work. Local,
project and header phases never overlap; their cap includes the calling watch worker.
Ordered gathering precedes first-valid-ID dedup and paired final sorting. Cancellation
settles every scoped lane before return; the existing watch owner keeps its publication,
accepted snapshot and bounded Drop/native lifetime contracts. Buffered JSON reads also
validate UTF-8 in ignored fields. Retained metadata size remains input-dependent.
The scanner shares the poster worker's Windows affinity detector: standard-library
parallelism can report the full host count under process affinity. Automatic scan
lanes are4–8,2 on one logical processor; optional top-level `catalog_scan_threads`
1–8 takes effect at restart. Policy logs once; opt-in phase traces report actual
activity and elapsed I/O work separately from serial discovery/startup/presentation.

For incorrect or missing posters, trace the current frame through `schedule_posters`, the worker mailbox, `Wake::PostersReady`, and GPU residency. Validate both selected/detail and pinned outgoing resources. Ordinary cards use up to 256 logical slots in a packed far atlas up to 2048×2048 with 160×88 tiles. The ordinary view promotes cards by selection, detail and displayed size. In Depth, selected, hovered and active detail cards qualify for the 512×512 near array. Other visible neighbors keep their far textures. The near array has six mips down to 16px. It grows in increments of eight up to 32 layers and stays within device limits. The same worker creates far tiles and handles near promotion or restoration. The GPU checks resource identity before remapping logical slots. The combined upload cap stays at 8 MiB per frame.

The worker handles cache barriers, far restores and current decode in that
order. Near restores wait for 100 ms of far/decode quiet, then take one
source group at a time. The renderer supplies the current ordered near demand.
Queued leases leave the worker when that demand changes, with their previous
pixel state restored. A moving Wall or Geometric camera holds fresh near work
outside dense entry prewarm and active detail. An active encode can finish. A clipped ordinary card can
reuse a resident near tile while new near work requires its centre in the
visible clip and sufficient opacity. A prewarm can settle at far quality for
those clipped cards. Queued near work counts as activity for maintenance,
colour analysis and cache pregeneration.

Within a demand batch, matching media kinds, paths and file fingerprints share a decode. A current reusable slot or roll pin can supply that result. Each logical slot retains its own upload state and incremental completion. Restore shares one canonical read or decode per path and captured recovery key within the restore batch.

On a compressed Depth surface, a restore request can already own the finished
160×88 RGBA tile. The worker packs those tiles directly to BC1 and asks for one
redraw if further work remains. The selected poster can therefore draw at far
quality alongside its neighbors while the worker prepares its BC7 mip chain.
Each slot checks its cache epoch again when publishing. Each far batch and near
source group also requests a redraw when it completes.

Gallery poster identity includes the path and media kind. Local video targets and
Wallpaper Engine video previews use the existing worker lanes. A Wallpaper Engine
entry without a usable preview or image still gets a deterministic placeholder
seeded by its target path. Image-only settings, colour analysis and export keep
their still selector.

`src/poster_worker/video.rs` opens Media Foundation only on the worker. It pairs
COM and native buffer lifetimes, checks dimensions and signed RGB32 stride, and
samples the visible display aperture into the canonical canvas. Media Foundation DLLs are optional
at launch. A versioned `video-poster-*.rgba` record contains a little-endian aspect
ratio followed by the 512² canonical RGBA bytes. The existing cache owner handles
its read, publication, clear and eviction. Extraction failure produces a path-based
fallback without a disk record. Restore reads the captured canonical record or
re-extracts the matching file stamp. Wanted checks run between synchronous native
calls. Windows controls when an in-flight codec call returns.

The compositor requests BC when the adapter offers it, using BC1 for far and BC7 for near. RGBA remains available on devices without BC. Set `WALLPAPER_SWITCHER_POSTER_RGBA=1` for an owned Preview process to exercise that fallback. The canonical 512×512 disk cache still supplies both paths.

Fresh catalog acceptance offers a bounded set of stamped image and video sources
to the poster worker. The existing below-normal colour lane fills missing canonical
records after colour work and a quiet poster interval. The pass writes only into
measured cache headroom and leaves GPU staging to gallery demand. Poster and
restore requests invalidate the quiet interval. The next slice measures headroom
again. Changed catalog revisions and cache clear invalidate outstanding work.

The worker keeps canonical `NearRgba` canvases separate from packed `NearRgbaMips` /
BC7 uploads. Mips average linear light; every level counts toward the8MiB upload
cap and survives GPU growth/compaction copies. Packed ready staging reserves part
of the existing decoder budget; reclaim prior batches before deferring current
priority work. Far tiles use compact physical slots and a2048-wide texture with
variable rows. Two seconds at no more than half the allocation triggers shrink;
retained resource identities survive physical-slot remapping. Slices, Wall and
Geometric keep unused logical slots and their far tiles. New demand uses empty
layers first, then the least recently requested unpinned layer. Source stamps
are checked on reuse. Retained far cards can release their canonical staging
pixels. Depth still retires unused far slots. Wall and Geometric add one adjacent
row or column on each side of the same layout window used for drawing and hits.
The travel side comes first, after the visible decode prefix. Allocation
diagnostics describe requested texture bytes.

An explicit Wall/Geometric request can hold target-scene visibility/input while its
existing worker and atlas prewarm. Terminal decode includes unavailable outcomes.
The GPU completion receipt binds poster/motion generations, selection, viewport and
the exact resource vector before the reducer releases entry. Invalidation cancels
that gate. Modifier bookkeeping remains active. One maintenance wake revisits idle
shrink; bounded16ms wakes pump pending GPU receipts without blocking the render
thread. Upload/fence completion is separate from actual presented-frame timing.


P5.5 presentation inputs (filtered IDs/aspects/index positions) are cached by the
existing catalog/filter revision owners. Unchanged refresh retains shared Arc
identity. Slices caches its sequential-f32 base strip and queries a sparse-width
window; Hex searches the original coarse column predicate; Wall shares immutable
placements and a projected-y candidate index before the same exact projection/clip.
Frame clamps and separate negative-gap navigation/draw bounds remain compatible.
Paint/focus/camera/hover/entrance and source/resource/loading identity stay dynamic.
Rail folder visibility tests emptiness without constructing the full folder list.
Geometry morphs rebuild when geometry changes; unchanged ticks avoid library visits.
Opt-in WALLPAPER_SWITCHER_LAYOUT_TRACE counters distinguish inputs/builds/candidates.
The existing generation-bound camera producer publishes terminal state then exits;
its idle state does not remove poster/colour reducer retries or GPU progress and
atlas-maintenance wakes. Trace exit records are separate from presented frames.

## Desktop, provider, and persistence

Use `src/candidate_app/runtime.rs` and `src/candidate_instance.rs` to trace launch policy, physical profile identity, and instance guards. The normal launch can apply wallpapers and install hooks. Native test runs use explicit `--preview` with an owned profile, selected through `WALLPAPER_SWITCHER_CONFIG_DIR` where applicable. `src/candidate_app/operations.rs` plans Apply and `src/coordinator.rs` owns serialized mutation and recovery. `src/candidate_app/persistence.rs`, `src/candidate_state.rs`, and `src/file_publication.rs` own persistence and publication.

Engine 2.8 writes the current scene through the Windows console. `engine_ipc/query_console.rs` runs only `getWallpaper` in a short-lived mode of the same executable, before application/profile startup. Its parent assigns the helper to a private kill-on-close job before releasing the stdin barrier. The helper obtains a blank windowless console from the system sort process, reaps that process after sending EOF, and runs the validated Engine query through the existing bounded child runner. The parent verifies the entire job has exited. A retained job stays in the coordinator's existing unsettled-child gate. Empty, ambiguous or lossy readback cannot confirm a wallpaper. [Diagnostics](PROVIDER-POSTER-DIAGNOSTICS.md)

The profile-local `catalog-snapshot-v1.json` is disposable derived data, separate from v4 state. It carries source metadata/dimensions/tags only. Cached items paint stale while scanning and cannot authorize automatic provider changes. Identical fresh acceptance clears staleness without invalidating reusable posters; delivery acknowledgement cannot persist a rejected scan. Accepted snapshot I/O stays outside the watcher mutex and drains after owner shutdown while the worker retains its lifetime.

## Settings, profiles, and compatibility

`src/candidate_app/view/settings/` owns Settings drafts, conditional-row reveal,
profile projection, shortcut capture, and save feedback. Gallery profile actions
remain mode-local and address profiles by stable mode and profile ID. Config-backed
application shortcuts stay with `Config` and its save owner. Reserved Config and
Escape bindings cannot be claimed by local shortcut capture. Preview refuses
Config publication. Next and Previous preset resolve a stable ID in the current
picker's displayed order, then use the same owner as a manual preset selection.
Older individual preset bindings are retained as inert compatibility data.

Same-gallery profile application captures the outgoing ordinary or Depth presentation
before geometry publication. It restores the captured roll, restarts the existing
motion owner, and reschedules visible poster demand. Library search, source filters,
folder selection, and favourites reuse the existing catalogue and filter owners.
Hand and Collection are documented contracts only until they have their own stable
identity, draw and hit model, poster-demand mapping, settings projection, v4
compatibility rules, transition owner, and focused Preview evidence.

## Verification

- Run `cargo fmt --all -- --check`, then `scripts/dev.ps1 -Action Check`, `-Action CompileTests`, and `-Action Test` for the stable automated gates. The Test action runs binary and Theme tests. The wrapper provides isolated paths and restores its environment.
- For guarded native tests, bind the executable, profile, and window. Use the release build for performance comparisons and preserve the baseline executable or its measurements before rebuilding to the same target path. Preview's flag policy is in `src/candidate_app/runtime.rs`.
- Use the tests in `src/candidate_app/tests/frontend_layout.rs` for existing dense GPU/atlas fixtures. Some tests are deliberately ignored and have explicit invocation and ownership requirements.
- Preserve [Skwd Wall attribution](third_party/SKWD-WALL-NOTICE.md), [font notices](../LICENSES/FONTS.md), [JPEG decoder provenance](third_party/EFFECTS-JPEG-DECODER.md), and test fixtures.
