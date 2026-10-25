# Builds the distributable archive for Windows into dist\.
# Usage: scripts\package.ps1 -Version 0.1.0 -Platform windows-amd64
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Platform = "windows-amd64"
)
$ErrorActionPreference = "Stop"
$name = "mugon-editor-$Version-$Platform"
$stage = "dist\$name\mugon-editor"

if (Test-Path "dist\$name") { Remove-Item -Recurse -Force "dist\$name" }
New-Item -ItemType Directory -Force "$stage\config" | Out-Null
foreach ($exe in "mugon-editor", "mugon-editor-thumbs", "mugon-editor-app") {
    Copy-Item "target\release\$exe.exe" $stage
}
Copy-Item "README.md" $stage
Copy-Item "config\mugon.example.toml" "$stage\config"
Compress-Archive -Path "dist\$name\mugon-editor" -DestinationPath "dist\$name.zip" -Force
Remove-Item -Recurse -Force "dist\$name"
Write-Output "dist\$name.zip"
