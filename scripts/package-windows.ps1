# Builds the Windows installer and the portable zip of Salak into dist\.
#
# Requires the Tauri CLI: cargo install tauri-cli --version "^2" --locked
# Run from anywhere:      powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot)

$version = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages[0].version

cargo tauri build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item "target\release\bundle\nsis\Salak_${version}_x64-setup.exe" dist

# The zip holds a single folder, so that extracting it does not spill files.
$folder = "target\portable\salak-$version"
Remove-Item -Recurse -Force target\portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $folder | Out-Null
Copy-Item target\release\salak.exe, README.md, LICENSE-MIT, LICENSE-APACHE $folder
Compress-Archive -Path $folder -DestinationPath "dist\salak-$version-windows-x64-portable.zip" -Force

Get-ChildItem dist
