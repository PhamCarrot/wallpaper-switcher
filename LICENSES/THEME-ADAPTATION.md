# Session-theme and role-editor attribution

The adopted theme_roles.rs/appearance_theme.rs and gpu_candidate.rs Theme editor reuse selected beta.24 palette role keys/document shape and grouped navigation labels/filter semantics. Lite owns RGB validation, reducer/lifecycle contexts, nine consumer mappings and session-only UI. The supplied beta.24 source is GPL-3.0-or-later; earlier beta.13 provenance is preserved in docs/third_party/SKWD-WALL-NOTICE.md (unchanged adopted input).

No generator, material-colors dependency, Matugen or external-app theme subsystem is imported. Wallpaper Switcher Lite 1.0 is distributed under GPL-3.0-or-later. Preserve this notice, the GPL text, font/OFL notices and source headers. The original MIT attribution is retained in MIT-KAT.txt.

## PK-10 starter palettes and Designer metrics

The 26 names and full two-variant documents in `assets/themes/starter-palettes.json`
are extracted from Skwd Wall v1.0.0-beta.26, commit
`780f2d3f53025318cbbaff1d1aa369e622e5e8fd`, and skwd-deck commit
`3e8df6dd517e676b6474c0e24bd9a4ce2d28fdc3`. The inputs are MIT-licensed,
copyright (c) 2025–2026 liixini. [MIT notice](MIT-LIIXINI-SKWD-BETA26.txt).

The extraction calls pinned Wall `src/domain/theme/candidate.rs::Candidate::from_preset`
with the stable keys in deck `crates/skwd-palette/src/presets.rs::PRESETS`.
The pinned deck `material/palette.rs` expands each primary into a Tonal Spot
document using Material Colors 0.4.2. The preset's background luminance selects
its native variant. The nine raw preset RGB values replace the corresponding
roles in that variant. The other 91 values retain the pinned expansion. The
bundled documents carry all 100 values in Lite's existing validated color format.
Material Colors notices are retained under [Apache 2.0](MATERIAL-COLORS-APACHE-2.0.txt)
and [MIT](MATERIAL-COLORS-MIT.txt). Lite ships the extracted data with no
Material Colors dependency or palette generator.

Palette selection opens the native variant as a clean, editable, unnamed Lite
draft after any required dirty decision. Applied snapshots and the built-in
Nord recovery document retain their existing ownership and bytes.

Designer typography, action padding and the picker follow pinned Wall
`src/frontend/theme_designer/view.rs` and
`src/frontend/ui/misc/{typography,folio}.rs`. The close button uses the existing
Lite Settings folio action. The earlier GPL adaptations and notices remain in
force for their original scope.
