[CmdletBinding()]
param([ValidateSet('Build','Check','Test','CompileTests','Preview')][string]$Action='Check')
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$names=@('TEMP','TMP','WALLPAPER_SWITCHER_CONFIG_DIR','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','CARGO_NET_OFFLINE')
$previous=@{}
foreach($name in $names){$previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
$location=Get-Location
try {
    Set-Location -LiteralPath $root
    $isolation=Join-Path $root ('target\isolated\'+[Guid]::NewGuid().ToString('N').Substring(0,8))
    New-Item -ItemType Directory -Path (Join-Path $isolation 't'),(Join-Path $isolation 'p') -Force | Out-Null
    $env:TEMP=Join-Path $isolation 't'
    $env:TMP=$env:TEMP
    $env:WALLPAPER_SWITCHER_CONFIG_DIR=Join-Path $isolation 'p'
    $env:CARGO_BUILD_JOBS='1'
    if([string]::IsNullOrEmpty($previous['CARGO_TARGET_DIR'])){
        $env:CARGO_TARGET_DIR=Join-Path $root 'target\build-cache'
    }
    $env:CARGO_NET_OFFLINE='true'
    switch($Action){
        Build { & cargo build --locked --offline --release --bin wallpaper-switcher-gpu-candidate }
        Check { & cargo check --locked --offline --bins }
        CompileTests { & cargo test --locked --offline --no-run --bins --tests }
        Test {
            & cargo test --locked --offline --bins -- --test-threads=1 --skip windows_wallpaper_host_is_discoverable
            if($LASTEXITCODE -ne 0){throw 'Non-desktop unit tests failed'}
            & cargo test --locked --offline --test theme_roles_contract
            if($LASTEXITCODE -ne 0){throw 'Theme contract tests failed'}
        }
        Preview {
            & cargo build --locked --offline --release --bin wallpaper-switcher-gpu-candidate
            if($LASTEXITCODE -ne 0){throw 'Preview build failed'}
            $binary=Join-Path $env:CARGO_TARGET_DIR 'release\wallpaper-switcher-gpu-candidate.exe'
            if(-not(Test-Path -LiteralPath $binary -PathType Leaf)){throw 'Preview build did not produce the selected executable'}
            # Default is fresh; explicit reuse still passes the application's marker/owner guards.
            if([string]::IsNullOrEmpty($previous['WALLPAPER_SWITCHER_CONFIG_DIR'])){
                Remove-Item -LiteralPath 'Env:\WALLPAPER_SWITCHER_CONFIG_DIR' -ErrorAction SilentlyContinue
            } else {
                $env:WALLPAPER_SWITCHER_CONFIG_DIR=$previous['WALLPAPER_SWITCHER_CONFIG_DIR']
            }
            $previewProcess=Start-Process -FilePath $binary -ArgumentList @('--preview') -WorkingDirectory $root -WindowStyle Hidden -PassThru -Wait
            $LASTEXITCODE=$previewProcess.ExitCode
        }
    }
    if($LASTEXITCODE -ne 0){throw "$Action exited with code $LASTEXITCODE"}
} finally {
    Set-Location -LiteralPath $location.Path
    foreach($name in $names){
        if($null -eq $previous[$name]){Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue}
        else{[Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')}
    }
}
