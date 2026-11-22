# Synthetic video poster fixtures

These five H.264 clips contain generated color fields. They carry no user media, local paths, Wallpaper Engine assets or audio.

| File | Test input | SHA-256 |
| --- | --- | --- |
| `red-blue.mp4` | Red over blue. Used for decoder and worker controls. | `5738d85a1e3cd78f4e3a3eb4139afbb4b7afd27c30e09fc5fcc9f695b9182864` |
| `green.mp4` | Solid green, also 1,610 bytes. The test restores the original modification time after replacing the source. | `edf5f18b8eb5990f29a16a8e42ba8e4efe4753f47bea64f7c8f83adf30b2881b` |
| `rotate.mp4` | 96×64 encoded with a +90° display matrix. The displayed pixels are red on the left and blue on the right. | `3198e98d5364b46c78fa891c475b69896625a12cd028e5696a15e683420ef8eb` |
| `anamorphic.mp4` | A split-color sample with 4:3 pixel aspect. | `ea70019fd7c5896a74682def85d9f3b4761a2b47e51959379c516cf63a08c185` |
| `delayed.mp4` | 24 packets precede the first marked keyframe. The test checks the pixels in the first usable frame. | `50696d627f77d466cbb741a29089d679fa149d6974ff1712b5790041007e61d0` |

Run these from the repository root in a disposable PowerShell session on Windows with Media Foundation H.264 decoding available. The tests use temporary copies, start no provider, and open no app window. Wallpaper Engine fixture coverage uses a synthetic Workshop directory and the gallery's poster worker.

```powershell
$fixturePath = (Resolve-Path 'tests/fixtures/video').Path
$replayPath = Join-Path (Get-Location) ('target/video-replay-' + [Guid]::NewGuid().ToString('N'))
$tempPath = Join-Path $replayPath 'temp'
$profilePath = Join-Path $replayPath 'profile'
New-Item -ItemType Directory -Path $tempPath,$profilePath | Out-Null
$env:TEMP = $tempPath
$env:TMP = $tempPath
$env:WALLPAPER_SWITCHER_CONFIG_DIR = $profilePath
$env:WALLPAPER_SWITCHER_VIDEO_FIXTURES = $fixturePath
$env:WALLPAPER_SWITCHER_P6_FIXTURE = Join-Path $fixturePath 'red-blue.mp4'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target/build-cache'
$env:CARGO_BUILD_JOBS = '1'
foreach ($filter in @(
    'video_equal_mtime_replacement',
    'native_video_metadata_and_delayed',
    'held_video_read_boundary',
    'p61_native_video',
    'p62_p63_native_engine_previews'
)) {
    cargo test --locked --offline --bin wallpaper-switcher-gpu-candidate $filter -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "$filter exited with code $LASTEXITCODE" }
}
```

Each filter should run one test. The validated codec host was Windows 11 Pro `10.0.26300` with `mfplat.dll` `10.0.26100.9949`, `mfreadwrite.dll` `10.0.26100.9818`, and `msmpeg2vdec.dll` `10.0.26100.9974` present. FFprobe from FFmpeg 9.0.2 inspected stream metadata and packets independently. FFmpeg is unnecessary to replay the tests. The held-boundary case holds a real decoded sample after the Windows call returns.
