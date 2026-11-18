# Wallpaper Switcher

A Windows wallpaper switcher for local pictures, videos, and installed
Wallpaper Engine projects. The gallery includes Slices, Wall and Geometric
layouts and uses a profile-local poster cache.

## Build on Windows

Use 64-bit Windows, the stable Rust MSVC toolchain, Visual Studio Build
Tools with Desktop development with C++ and a Windows SDK, and PowerShell 7.
The development script uses the locked dependencies in offline mode.
Populate the Cargo cache on the first checkout with internet access.

```powershell
cargo fetch --locked
pwsh -NoProfile -File scripts/dev.ps1 -Action Check
pwsh -NoProfile -File scripts/dev.ps1 -Action CompileTests
pwsh -NoProfile -File scripts/dev.ps1 -Action Test
pwsh -NoProfile -File scripts/dev.ps1 -Action Build
```

The default build output is
`target/build-cache/release/wallpaper-switcher-gpu-candidate.exe`.

## Try an isolated Preview

```powershell
pwsh -NoProfile -File scripts/dev.ps1 -Action Preview
```

Preview opens a separate profile with wallpaper Apply and global hooks
disabled. With `WALLPAPER_SWITCHER_CONFIG_DIR` unset, the launcher selects
a fresh profile. To reuse an isolated Preview, point that environment
variable at an existing absolute folder with this marker in
`candidate-launch-mode.toml`.

```toml
version = 1
mode = "preview"
```

Open Settings → Library → Manage library to add a local folder. The
[example configuration](config.example.toml) includes the catalog and
cache settings. The [architecture notes](docs/ARCHITECTURE.md) describe
the worker and cache behavior.

## License and credits

Wallpaper Switcher is distributed under the [GNU GPL v3](LICENSE).
This project credits [skwd-wall](https://github.com/liixini/skwd-wall).
See the [reuse notice](docs/third_party/SKWD-WALL-NOTICE.md),
[bundled font licenses](LICENSES/FONTS.md), and
[Windows dependency notices](LICENSES/dependencies/INDEX.md).
