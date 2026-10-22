# Builds the distributable archive for Windows into dist\.
# Usage: scripts\package.ps1 -Version 0.1.0 -Platform windows-x86_64
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Platform = "windows-x86_64"
)
$ErrorActionPreference = "Stop"
$name = "mugon-editor-$Version-$Platform"
$stage = "dist\$name\mugon-editor"

if (Test-Path "dist\$name") { Remove-Item -Recurse -Force "dist\$name" }
New-Item -ItemType Directory -Force "$stage\config" | Out-Null
foreach ($exe in "mugon-editor", "mugon-editor-thumbs", "mugon-editor-app") {
    Copy-Item "target\release\$exe.exe" $stage
}
# FBX2glTF is not part of the repo; it is downloaded (and cached in dist\.cache) when packaging.
$fbxVersion = "v0.9.7"
$fbx = "dist\.cache\$fbxVersion-FBX2glTF.exe"
if (-not (Test-Path $fbx)) {
    New-Item -ItemType Directory -Force "dist\.cache" | Out-Null
    Invoke-WebRequest -Uri "https://github.com/facebookincubator/FBX2glTF/releases/download/$fbxVersion/FBX2glTF-windows-x64.exe" -OutFile $fbx
}
Copy-Item $fbx "$stage\FBX2glTF.exe"
Copy-Item "README.md" $stage
Copy-Item "config\mugon.example.toml" "$stage\config"
Compress-Archive -Path "dist\$name\mugon-editor" -DestinationPath "dist\$name.zip" -Force
Remove-Item -Recurse -Force "dist\$name"
Write-Output "dist\$name.zip"
