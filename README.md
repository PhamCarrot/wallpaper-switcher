# Wallpaper Switcher

A wallpaper switcher for Windows. Browse your wallpapers in a fast GPU
gallery and switch the desktop with animated transitions — all from the
system tray.

## Supported wallpapers

- **Pictures** — JPG, PNG, BMP and other common image formats
- **Videos** — MP4, MKV, MOV, AVI, M4V, WebM
- **Wallpaper Engine** — installed Steam Workshop projects

## Features

- Four gallery layouts: Slices, Wall, Geometric and Depth
- 30 transition effects, animated on the GPU; effects that need a static
  image fall back to a crossfade on animated wallpapers
- A disk cache that keeps gallery posters ready between launches
- A startup loading screen, a theme designer, playlists and a scheduler
- Tray-first design: the app can live in the tray and apply wallpapers
  without opening a window

## Getting started

Requires 64-bit Windows, the stable Rust MSVC toolchain, Visual Studio
Build Tools (with "Desktop development with C++"), a Windows SDK, and
PowerShell 7.

```powershell
# Fetch the locked dependencies once, with internet access
cargo fetch --locked

# Build
pwsh -NoProfile -File scripts/dev.ps1 -Action Build
```

The binary is written to
`target/build-cache/release/wallpaper-switcher-gpu-candidate.exe`.

To look around without touching your desktop, run the isolated Preview
(it uses a separate profile with wallpaper Apply and global hooks off):

```powershell
pwsh -NoProfile -File scripts/dev.ps1 -Action Preview
```

Then open **Settings → Library → Manage library** to add your picture
and video folders. Wallpaper Engine projects are found automatically.
See [config.example.toml](config.example.toml) for all options.

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md)
for the build/test workflow, and these docs for how the app works inside:

- [How posters work](docs/HOW-POSTERS-WORK.md) — the gallery and cache pipeline
- [Architecture](docs/ARCHITECTURE.md) — components and safety rules

## License and credits

Wallpaper Switcher is distributed under the [GNU GPL v3](LICENSES/GPL-3.0-or-later.txt).

Parts of the gallery are adapted from
[skwd-wall](https://github.com/liixini/skwd-wall) — thank you to its author.
See the [reuse notice](docs/third_party/SKWD-WALL-NOTICE.md),
[bundled font licenses](LICENSES/FONTS.md) and
[dependency notices](LICENSES/dependencies/INDEX.md).
