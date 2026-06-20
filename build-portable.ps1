$ProjectPath = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoPath = Split-Path -Parent (Split-Path -Parent $ProjectPath)
$ReleasePath = Join-Path $RepoPath "releases\link-downloader"

cd $ProjectPath

Write-Host "Building Link Downloader..." -ForegroundColor Cyan

npm install
if ($LASTEXITCODE -ne 0) {
    Write-Host "npm install failed." -ForegroundColor Red
    exit 1
}

npm run tauri build
if ($LASTEXITCODE -ne 0) {
    Write-Host "Tauri build failed." -ForegroundColor Red
    exit 1
}

New-Item -ItemType Directory -Path ".\portable" -Force | Out-Null
New-Item -ItemType Directory -Path $ReleasePath -Force | Out-Null

$SourceExe = ".\src-tauri\target\release\bulk-link-downloader.exe"
$PortableExe = ".\portable\link-downloader.exe"
$ReleaseExe = Join-Path $ReleasePath "link-downloader.exe"

if (-not (Test-Path $SourceExe)) {
    Write-Host "Release executable not found:" -ForegroundColor Red
    Write-Host $SourceExe -ForegroundColor Red
    exit 1
}

Copy-Item $SourceExe $PortableExe -Force
Copy-Item $SourceExe $ReleaseExe -Force

Write-Host ""
Write-Host "Portable EXE created:" -ForegroundColor Green
Write-Host $PortableExe -ForegroundColor Green
Write-Host ""
Write-Host "Release EXE copied to:" -ForegroundColor Green
Write-Host $ReleaseExe -ForegroundColor Green
