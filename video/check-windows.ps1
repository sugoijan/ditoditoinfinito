# Opt-in validation on a real Windows GPU. No window, sound or saved settings.
# The module can be built on another host; supply its unchanged .wasm file.
param([Parameter(Mandatory=$true)][string]$Module)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'This runner requires Windows' }
$root = Split-Path $PSScriptRoot -Parent
$modulePath = (Resolve-Path $Module).Path
$report = Join-Path $root ('target/scratch/windows-video-report-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Force $report | Out-Null
$names = @('DDI_HOME', 'DDI_VIDEO_CACHE', 'DDI_VIDEO_MODULE')
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
Push-Location $root
Start-Transcript -Path (Join-Path $report 'run.log') | Out-Null
try {
    $env:DDI_HOME = Join-Path $report 'home'
    $env:DDI_VIDEO_CACHE = Join-Path $report 'cache'
    $env:DDI_VIDEO_MODULE = $modulePath
    Get-FileHash -Algorithm SHA256 $modulePath
    Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture | Format-List
    Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, DriverDate, PNPDeviceID | Format-List
    & rustc -Vv
    if ($LASTEXITCODE -ne 0) { throw 'rustc failed' }
    & ffmpeg -version
    if ($LASTEXITCODE -ne 0) { throw 'ffmpeg with libx264, libx265 and libvpx-vp9 must be on PATH' }
    & cargo run -q -p xtask -- gen-songs
    if ($LASTEXITCODE -ne 0) { throw 'gen-songs failed' }
    & cargo run -q -p xtask -- gen-noteskins
    if ($LASTEXITCODE -ne 0) { throw 'gen-noteskins failed' }
    & cargo test -p ddi-desktop --test video_media_foundation -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Windows video fidelity validation failed; retain the report and generated clips' }
    'Fidelity validation passed. Device-loss, adapter-switch, performance and process-isolation checks remain manual.'
} finally {
    Stop-Transcript | Out-Null
    Pop-Location
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    Write-Host "Report: $report"
}
