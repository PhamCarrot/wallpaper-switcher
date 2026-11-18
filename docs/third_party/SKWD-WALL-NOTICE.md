# skwd-wall frontend reuse notice

The dated entries record the original adaptation scopes. Wallpaper Switcher retains the attribution for those adaptations and their shared code and assets.

## Selected shared frontend resources and projection — P3, 2026-10-16

The supplied read-only skwd-wall-2 locales are embedded under assets/locales. Source UI/script font subsets are copied as assets/fonts/RobotoCondensed-Bold-{UI,SC,JP,AR,BN,HI}.ttf. Existing original Bold/Nerd fonts and notices remain. The font license is [SKWD-OFL-1.1.txt](fonts/SKWD-OFL-1.1.txt). Existing GPL attribution/license text applies to supplied translation/presentation adaptations. This is not release or distribution acceptance.

Private candidate_app/localization.rs follows source src/i18n/catalog.rs and consumed script fonts in src/shell/shell.rs. Catalog selection remains app-owned. Fixed-locale static text caching supports source Card-title lifetimes without global selected-language state. Cached Fluent0.17 compiles offline; no localization service or font installation is used.

The pure model/row builder/table/tab composition, input parser/scope/slot model and declarative metadata under candidate_app/view/settings/composed adapt supplied frontend/settings, domain/input and contracts/picker; key/default/bounds metadata follows sibling skwd-config. Only data/presentation slices are included; no reference App, config IO, daemon clients or second saver. Public Sandy/Hand/Collection branches are excluded under selected scope. Explicit-locale helper parameters, stable-target display observation requirements and unavailable Windows app-theme discovery are local adaptations. Legacy app data and actual provider ownership remain Lite-owned.

The private settings/folio workbench/control/background/display/app-theme views and donor_folio/source_ui typography/layout/styles/direction/animated buttons/slider/pane/studio/color leaves adapt the consumed beta.24 frontend/settings/folio and frontend/ui source. They retain the existing GPL attribution and license. Pure app-owned intents replace source App messages; locale/direction are explicit, preview and per-display intents remain distinct, and unavailable execution stays disabled. The generic redraw clock and application controller are Lite integration code; no source daemon or application updater is imported.

Exact raw assets, source manifests, adaptation records, scratch failures, executed test artifacts and review findings/repairs are under the historical development archive. Asset intake binds580 resources and pre-copy absences; projection intake binds38 initial private source files; folio-integration/copy-manifest.json binds consumed renderer/leaves and namespace adaptation. Source tests and intermediate compilation do not establish completed forms, localization, native interaction or visual fidelity.

## Selected beta.24 gallery presentation

The private `candidate_app/{gallery_presentation,hex_presentation,wall_presentation,detail_motion,ordinary_roll}.rs`
and connected `gallery`, `gpu_gallery`, `donor_scene`, `donor_back_view`, and `donor_rail` paths selectively adapt
the supplied beta.24 `app/scene/core/{view_slices,view_hex,view_grid,view_depth,detail,tick,reel_state,selection,lifecycle}.rs`,
`app/runtime/position.rs`, `app/view/chrome.rs`, `frontend/animation`, and
`frontend/ui/bar/model/builder.rs`. These adaptations remain under the existing
GPL-3.0-or-later attribution/license. The supplied source is read-only at
`skwd-wall-2`.
Lite retains its application, current-frame/stable-ID ownership, guarded Windows
input/lifetime, bounded worker/mailbox/atlas, local publisher, and sole provider
coordinator. No upstream application/daemon or additional renderer is imported.
Current source/test/native acceptance is recorded separately in STATUS/PLAN.

This project contains narrow source adaptations of the skwd-wall presentation.

- Project: skwd-wall
- License: GPL-3.0-or-later
- Pinned upstream release: `v1.0.0-beta.13`
- Pinned commit: `9735be4ee1cf8b367773170e9e24c4f07ec3dd8b`
- Read-only upstream: `reference\skwd-wall`
- Read-only Windows adaptation: `prototypes\skwd-wall-windows-upstream`

Adapted Lite presentation modules include:

| Lite path | Donor source/contract |
|---|---|
| `src/upstream_slices_adapter.rs` | Pinned Slices filtered-window geometry and camera behavior. |
| `src/donor_layout.rs` | `frontend/scene/layout.rs` and the data-only Grid/Hex placement paths. |
| `src/donor_scene.rs` | Donor rendering snapshot/item payload and card-flip handoff contracts. |
| `shaders/skwd_item.wgsl` | Pinned donor item shader, with documented Lite compatibility fields. |
| `src/donor_ui.rs` | `frontend/ui/misc/{folio,layout,style,typography}.rs` metrics and roles. |
| `src/donor_detail.rs` | `frontend/ui/chrome/back/geometry.rs` affine points/inverse points and `app/scene/core/detail.rs` separate external source-to-target body calculation. |
| `src/donor_back_layout.rs` | `frontend/ui/chrome/back/layout.rs` embedded wide/stacked and external layout/wrapping, adapted for supported Lite facts/actions and deliberate content overflow. |
| `src/donor_back_view.rs` | `frontend/ui/chrome/back/{view,background,content,actions}.rs` drawn surfaces, text angle, staged reveal, and inverse action hits; Lite explicit Apply and close controls. |
| `src/donor_rail.rs` | `frontend/ui/bar/canvas.rs`, `frontend/ui/bar/view.rs`, and related FilterBar geometry/interaction concepts, adapted to Lite intents. |
| `src/donor_folio.rs` | `frontend/ui/misc/{folio,layout,style}.rs`, settings/folio/folio.rs index/tree/reading structure; supported Lite Search/Import/organization/automation/appearance/Engine content only. |
| `src/donor_search.rs` | Pinned `app/view/tags.rs` compact cloud sizing/anchor and `app/view/chrome.rs` gallery footprint; `frontend/tagcloud/tagcloud.rs` and `ui/chips.rs` search field, tag controls, bounded rows, read-only include/exclude interaction. Lite keeps title/ID/folder matching and providers. |

Task 2 rail correspondence also uses pinned `frontend/ui/bar/model/builder.rs`, `app/view/chrome.rs`, and the bounded Fast Tween calculation in `frontend/animation/{animation,motion}.rs`. Lite retains its existing detail visibility policy and window lifecycle; the new rail animation does not import SceneCore or change wallpaper ownership.

Lite retains stable `WallpaperItem.id` values, static poster decoding/cache ownership, Iced widget/content composition, accessibility/messages, window/lifecycle state, persistence, automation/import, `WallpaperCoordinator`, native and Wallpaper Engine providers, recovery, and explicit Apply.

The donor daemon, IPC, Linux shell, WindowsHost/TCP/toggle helpers, provider ownership, startup services, theme designer, atlas/live-preview stack, Sandy/sand, online/AI/download, deletion, and unsupported metadata actions are not included.

The Windows prototype is supporting evidence and has no local Git binding to the pinned commit. Its packaged renderer was recorded as byte-identical to its prototype release executable, but byte-identical executables do not attest the entire source tree/toolchain. When supporting Windows material disagrees with the pinned upstream behavior, the pinned commit is authoritative.

## Packed poster tiles and compression · P1.4

The packed far atlas and capability-gated BC1/BC7 choices follow the supplied skwd-wall beta.24 rendering and encoding designs (`src/rendering/scene/atlas.rs` and `src/contracts/preview/encoding.rs`). Wallpaper Switcher uses its own slot identity, worker, and runtime poster preparation. The Iced 0.14 wgpu compositor is vendored from crates.io (`3997291f318a8bc06fa522f5579836fb3feb94df`) with one local patch in `src/window/compositor.rs` to request the optional BC feature and retain the RGBA device fallback. Its MIT license is in `vendor/iced_wgpu/LICENSE`. The encoder uses locked `intel_tex_2` 0.5.0, with notices in `LICENSES/dependencies/`.

Preserve this notice, the pinned identity, applicable GPL source obligations, and the project's other license/font notices in packages containing these adaptations.

## Narrow beta.24 Depth adaptation — D1, 2026-10-09

The new internal Depth algorithms only (not the pre-existing modules as a whole)
adapt the supplied selected beta.24 source under
`skwd-wall-2`.
License remains GPL-3.0-or-later; a shared commit or Windows build is not established.
Existing beta.13 notices and attribution remain in force for their original code.

| Source file | SHA-256 | Narrow use |
|---|---|---|
| `src/app/scene/core/view_depth.rs` | `9272A87BAB335C837B916514785E0AA3E78A8AF4E3F6101DDEC43C7270FB7ED3` | Independent width/spacing, logarithmic offset, fit, bounded candidates, decay/fade and stable far-to-near order in donor_layout.rs. |
| `src/frontend/scene/layout.rs` | `75B1667FC580C72B31F8D95B7F4E03F9111303C30CC996F144FA7CDB74830B96` | Fitted skew/corner policy, reusing the existing Lite affine/corner helpers. |
| `src/app/scene/core/layout_helpers.rs` | `385BD14795FFA4E1F4182E733A635A8B2E0744E3CF41D6AB88DEEEF2A5897330` | Position-driven zoom/pan, mapped through Lite's decoded content bounds in gpu_gallery.rs. |
| `src/frontend/animation/animation.rs` | `FB054B1373558E24C2EB96199040FDCD59987AC7523CDF7B3A7C573B592EA5B9` | Absolute target/retime/precision policy in the existing GalleryMotionState; critical-damped equations already present in Lite. |

D1 adds a Depth-only rounded hit refinement matching the current sheared mask,
and a target-relative fractional remainder to avoid large-index f32 integration
stall. It does not import SceneCore, workers, atlases, live previews, providers,
application event wiring, a new shader path or persistent state.

## Narrow Depth state definitions — D2, 2026-10-09

The independent mode-owned serializable definitions in candidate_state.rs adapt
the approved beta.24 schema/catalog values only; no rendering or UI is added.
Lite TOML migration/snapshots remain Lite-owned. Existing attribution is retained.

| Read-only supplied source | SHA-256 | Narrow use |
|---|---|---|
| `skwd-wall-2\src\contracts\settings\catalog.rs` | `AF6EAB00B4C3DA690E939B096152415BBAAAC2626EBC500B9F606CDD43CE6D4C` | Twelve stable effect names and explicit shader ID mapping. |
| `skwd-deck-main\crates\skwd-config\src\schema.rs` | `D9969EBE3BA7B3F243E9E5F9843E4876AB488F52E27277B3B22D4BAB8919904A` | Depth geometry/detail defaults and ranges, adapted as independent Lite state. |

## Narrow default Depth integration — D3, 2026-10-09

D3 connects the existing D1 Depth geometry/camera and D2 mode-owned state to Lite's
normal application, gallery widget, renderer and static-poster owner. The supplied
selected beta.24 directory remains read-only and has no established shared commit
or Windows-build identity. Existing beta.13 attribution and D1/D2 file hashes above
remain in force; this entry does not relabel the older modules as beta.24.

Additional narrow source contracts under `skwd-wall-2`:

| Source file | SHA-256 | D3 adaptation boundary |
|---|---|---|
| `src/app/scene/core/selection.rs` | `A9A4082EC8C65CBB19E8B323EBA4FCAAD162B99234EA09BBE07B385101186384` | Absolute-camera navigation and Depth wheel policy in the existing Lite app owner. |
| `src/app/scene/core/view_slices.rs` | `2EA13CF4BA367FEBDD7C49F58CF1947F4CB8D4B034772F9268A98B54A76584C7` | Default current/noncurrent/hover card presentation, adapted to Lite theme colors. |
| `src/app/scene/core/render_model.rs` | `EE550F909E46E619018415D7CEEEAEE4E5D0889C606584D32682D44445E3CABB` | Existing affine item/body payload and current-card chrome correspondence. |
| `src/app/update/picker.rs` | `AB6D73FD915D65FAFEB2A9783ECC304153B22C4A03E4F49E16BD3245A54E6483` | Two-stage selection/detail pointer policy; Lite browsing never applies. |
| `src/domain/input/action.rs` | `82DDA2806A4069FA8D9C8174CF493CE193021D9779EDC60B581565AD81553CE5` | Depth navigation/right-click intent correspondence through Lite-owned input capture. |
| `src/app/scene/core/detail.rs` | `D740C15ADBE8E11A0A9DFF65AF2F1F470DE516A2513C07D2002CB8E10E3F573B` | Current fitted card identity/geometry for the existing supported inline detail surface. |

The latest-completion mailbox, profile transaction safeguards, catalog identities,
32-layer static-poster atlas, decoder, worker/event lifecycle and explicit Apply
remain Lite-owned. No donor daemon, live video, provider action, external overlay
owner or new renderer is imported. D3 is a default source slice: full Depth
settings/detail-effect/filter-roll correspondence remains D4 work; executed GPU/UI,
resource measurements and human acceptance remain separately authorized later work.


## Narrow Depth controls/style/detail — D4a, 2026-10-10

D4a extends the existing Lite-owned D1/D2/D3 paths with all fourteen mode-owned
controls, current-to-target geometry approach, navigation/detail retiming, stable
effect/flag payloads, body-local shadows, and current-frame inline detail/input.
The beta.24 source and GPL attribution above remain read-only and unchanged.

The approach/equality contracts reuse `src/frontend/scene/layout.rs` above;
selection/navigation retiming follows `src/app/scene/core/lifecycle.rs`; shadow
offsets/alpha and style follow `src/app/scene/core/view_slices.rs` above. Detail
phase behavior remains the existing `src/frontend/scene/card_flip.rs` contract.
All twelve shader effect branches already exist in Lite's attributed shader;
no shader ABI or desktop transition rewrite occurs in D4a. Depth-only rounded
back/input correspondence and stable revision guards remain Lite refinements.
The current card remains undimmed throughout selection interruption per the
approved Lite contract; noncurrent tint follows the donor selection spring.

D4b filter roll/ghosts, D5 integrated source acceptance, and separately authorized
GPU/UI/resource/human acceptance remain pending. D4a source checks do not attest
rendered effect fidelity, physical input, latency, or a donor Windows build.


## Narrow Depth filter roll — D4b, 2026-10-10

Adapted only the selected source-led filter roll through Lite's existing motion,
frame and poster owners. Existing beta.13 and D1–D4a notices remain in force;
the supplied beta.24 directory remains read-only without an invented commit.

| Read-only source under skwd-wall-2 | SHA-256 | Narrow adaptation |
|---|---|---|
| `src/app/scene/core/reel_state.rs` | `3A76497DABD603D4CF9D8A62F92B488C3B776B0FD1B15DA2A3915F812B69EACE` | Per-card -0.6*x/viewport stagger, donor cell matching and same-identity reuse; Lite bounded presented-history capture. |
| `src/app/scene/core/tick.rs` | `B8A6486705129C3255FDEF328CD85E146C73C06A406DCC6D4E70A4FEC15F93F9` | Capped 50 ms progression and minimum 50 ms duration, adapted into existing Lite motion generation. |
| `src/frontend/scene/reel.rs` | `AEE45FDC6CA781936C03CE180BDFB99DA0B80172D4D30A199ED9F82F83A66C13` | Incoming/outgoing half-width, center, crop and radius cut equations applied once to Lite current frame. |
| `src/frontend/scene/reel/tests.rs` | `76D630F03465103286C57D3237139E4AA7B094E50D8F6E65DE4197FBC500C0A5` | Independent donor tiling check; Lite adds content-relative crop, clipped rounded hits and exact resource tests. |
| `src/app/scene/core/view_depth.rs` | `9272A87BAB335C837B916514785E0AA3E78A8AF4E3F6101DDEC43C7270FB7ED3` | Far-to-near live stream followed by outgoing bodies, each retaining its own painter order. |

Lite refinements are one cut frame for rendering/hits/detail/demand; omitted
subpixel bodies, AA-excluded raster-quad hits at extreme skew cuts, visual-only
outgoing bodies with exact resource identity, and a single 32-layer atlas.
Upcoming current-window preload is retained in that same frame; exact old pins
follow live priority, then bounded adjacent prefetch. The current live layout
never shrinks for resource pressure. Union>32 or presented-history>32 explicitly
ends outgoing retention with an opt-in trace/counter; incoming roll continues.
Interrupted history snapshots the currently presented geometry in live-then-old
order, using donor first-cell deduplication and same-cell identity reuse. Missing
old decode/upload is omitted, never replaced by a different layer's image;
missing live upload uses the existing solid placeholder. New catalog revision
cancels outgoing ownership even for reused ID/path, while eligible incoming roll
continues. Geometry/style/detail edits or viewport change cancel old cuts and
present current live geometry, preserving D4a springs/detail targets.

The single existing decoder/mailbox remains unchanged. Successful exact GPU
upload ownership is tracked inside the existing atlas pipeline; this introduces
no second atlas or decoder. Source tests/shader validation do not establish
executed GPU output, physical input, measured resources or human acceptance.
D5 integrated source/provenance review and D6 runtime/human gates remain pending.

## Integrated Depth source boundary — D5, 2026-10-10

D5 traces the joined D1–D4b geometry, motion, state, controls, detail, filter-roll,
input and exact static-poster ownership paths. The historical beta.13 commit and
the selected supplied beta.24 file-hash adaptation map above remain distinct.
Fresh reference checks bind the actual selected files, not the whole upstream
suite to a shared commit or Windows build. No additional donor subsystem,
dependency, decoder, atlas or provider owner is imported.

The bounded integration corrections are Lite-owned: fit supported Depth inline
back content and its drawing/hit/scroll metrics to tiny current card geometry;
anchor the final diagnostic screenshot selection before requested detail; apply
existing cancellation/invalidation owners at benchmark mode transitions; and
honor current window identity/hidden intent in late open callbacks. Existing
old-mode back layout and all desktop transition inputs are preserved. Very small
fitted content can reduce text/control size; source reachability does not assert
human legibility or physical-input acceptance.

The complete selected source gate is documented in the D5 evidence directory
linked by STATUS/PLAN. It does not accept rendered effect/crop/roll fidelity,
native lifecycle or input, measured resources, a matching Depth showcase segment,
human judgment, providers, packages or release. D6 is prepared only and requires
separate authorization; no runtime acceptance transfers from earlier artifacts.

### D6-R1 user-approved readable Depth panel adaptation — 2026-10-10

Lite now resolves readable inline versus existing external BackPanel once in the
current identity-bound detail owner. Complete presented rounded/sheared/cut
control rectangles and readable logical content metrics determine inline fit;
undersized sessions use normal panel text/control sizes and facts scrolling.
Panel selection latches through reversal/closing; inline may promote once with
stale press/revision invalidation. Gallery geometry/settings/crop stay unchanged.
Depth's panel remains target-sized during its existing shader/back reveal phases;
old modes keep their original external travel/inline layouts. This supersedes
only the tiny always-inline D5 adaptation above, preserving historical evidence.
No donor subsystem, schema, dependency, renderer or provider owner is added.
New source/runtime/review evidence is separately bound by current STATUS/PLAN;
prior D5/D6 source and binary acceptance does not transfer to changed bytes.

### Static-image Effects first processing slice — 2026-10-13

Offline Brightness/Invert RGB arithmetic and ordered execution are adapted from
skwd-deck-main/crates/skwd-wall-effects beta.24, GPL-3.0-or-later.
Only these two algorithms are implemented here; the approved 34-effect
roadmap is retained in EFFECTS-CONTRACT.md and the project PLAN. No GUI,
auto-Apply, permissive parameter fallback, Rayon/helper daemon, source rewriting
or overwriting output behavior was copied. Lite validates PNG8 RGB/RGBA,
preserves alpha/dimensions and exclusively creates requested output.

Reference raw-byte hashes (read-only source binding, not a Git commit):
- `builtin.rs`: `37C6BC474802AD7C51DEE2842D58D2017139D7D70F1BD96B9DD2D63B6594A0D3` (7653 bytes), `skwd-deck-main\crates\skwd-wall-effects\src\native\builtin.rs`.
- `render.rs`: `24B03C394124598A8A1D4048B43911F2EA3C33109410F3167C0A0FD249F87FE3` (2003 bytes), `skwd-deck-main\crates\skwd-wall-effects\src\native\render.rs`.
- `LICENSE`: `3972DC9744F6499F0F9B2DBF76696F2AE7AD8AF9B23DDE66D6AF86C9DFB36986` (35149 bytes), `skwd-deck-main\LICENSE`.

Current LICENSES/GPL-3.0-or-later.txt contains the same GPL text with CRLF
bytes; it is preserved. The upstream LICENSE uses LF bytes. User instructed
that this remains private personal use and no additional license copy is needed.
Existing notices and license material remain unchanged except this attribution
append. No public distribution or license-compatibility decision is made.

## In-app Effects preview slice (2026-10-13)

Preview stage, title/action structure and ordered numeric-control presentation were selectively adapted from selected skwd-wall beta.24 `src/frontend/effects/presentation/stage.rs`, `controls.rs` and `dock.rs`. Lite reuses its donor folio/styles and Iced image consumer; CPU processing, immutable snapshots, ordered duplicate/reorder session entries, cancellation, revision ownership and bounded shared worker/mailboxes are Lite-owned. Only Brightness/Invert are available. No donor daemon/RPC, shader approximation, Save/export/Apply, or donor cache cleanup is included. Existing license/provenance texts are preserved.


## PK-06 beta.26 Search composition and tag vocabulary (12 November 2026)

The Search composition and co-occurrence predicate selectively follow skwd-wall beta.26 commit 780f2d3f53025318cbbaff1d1aa369e622e5e8fd, src/frontend/tagcloud/tagcloud.rs, src/app/view/tags.rs and src/domain/library/search/search.rs under the existing GPL-3.0-or-later adaptation notice. Lite uses its existing title, ID, folder and tag query, its Windows eligibility and browsed playlist scope, and removable active zero-count tags.

## PK-01 beta.26 sort semantics and independent analysis (6 November 2026)

`library/sort_metrics.rs` selectively adapts the13-bucket analysis and thumbnail preparation from skwd-deck commit3e8df6dd517e676b6474c0e24bd9a4ce2d28fdc3, crates/skwd-wall-core/src/infrastructure/media/images.rs. Its upstream MIT notice is reproduced below. Seven browse comparator meanings and glyph identities follow skwd-wall beta.26 commit780f2d3f53025318cbbaff1d1aa369e622e5e8fd, domain/library/filter/filter.rs and frontend/ui/bar/catalog.rs, under the existing GPL-3.0-or-later presentation adaptation notice. Windows confirmed history, unavailable-last/stable-ID tie behavior, bounded worker/cache and W5 alpha-aware values remain Lite-owned adaptations. No daemon/provider or thumbnail renderer is imported.

MIT License

Copyright (c) 2025-2026 liixini

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Selected beta.26 Theme Designer presentation and palette data

PK-10 adapts the selected Designer metrics and picker from Skwd Wall
`v1.0.0-beta.26` (`780f2d3f53025318cbbaff1d1aa369e622e5e8fd`) and embeds
the extracted 26 palettes from its pinned skwd-deck palette source
(`3e8df6dd517e676b6474c0e24bd9a4ce2d28fdc3`). The original MIT and Material
Colors notices and the data conversion are recorded in
[THEME-ADAPTATION.md](../../LICENSES/THEME-ADAPTATION.md).
