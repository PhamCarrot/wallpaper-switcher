# Contributing

Thanks for helping improve Wallpaper Switcher.

## Set up

You need 64-bit Windows, the stable Rust MSVC toolchain, Visual Studio
Build Tools ("Desktop development with C++"), a Windows SDK and
PowerShell 7.

```powershell
cargo fetch --locked
pwsh -NoProfile -File scripts/dev.ps1 -Action Check
```

## Make a change

- Keep changes small and go through the existing owners described in
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- Never apply wallpaper or touch desktop state from browsing, selection,
  filter, detail or theme code — that belongs to `WallpaperCoordinator`.
- Keep `--preview` fully isolated (separate profile, no Apply, no global
  hooks) and keep the v4 state schema backward compatible.
- Don't commit secrets or machine-specific files. Agent working notes
  and `target/` are already ignored.

## Verify

```powershell
pwsh -NoProfile -File scripts/dev.ps1 -Action Check          # compile
pwsh -NoProfile -File scripts/dev.ps1 -Action CompileTests   # build tests
pwsh -NoProfile -File scripts/dev.ps1 -Action Test           # run the suite
cargo fmt --all -- --check                                   # formatting
pwsh -NoProfile -File scripts/dev.ps1 -Action Preview        # try it safely
```

Run tests with an isolated profile (`WALLPAPER_SWITCHER_CONFIG_DIR`
pointed at a scratch folder). Tests marked `ignored` that touch the real
desktop should only be run individually by name.

## Submit

Open a pull request with a short summary of what changed and the checks
you ran. Include screenshots for visible changes.
