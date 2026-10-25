# Installs the Mugon Editor through the mugon CLI (installing the CLI first if needed).
# Served at https://tools.mugon.net/editor/install.ps1.
#   $env:MUGON_TOOLS_URL  tools host (default https://tools.mugon.net)
$ErrorActionPreference = 'Stop'

$toolsUrl = if ($env:MUGON_TOOLS_URL) { $env:MUGON_TOOLS_URL.TrimEnd('/') } else { 'https://tools.mugon.net' }
$mugon = Get-Command mugon -ErrorAction SilentlyContinue
$fallback = "$env:USERPROFILE\.mugon\bin\mugon.exe"

if (-not $mugon -and -not (Test-Path $fallback)) {
    Write-Host 'Installing the mugon CLI first...'
    Invoke-Expression (Invoke-RestMethod "$toolsUrl/cli/install.ps1")
}

$mugonPath = if (Get-Command mugon -ErrorAction SilentlyContinue) { 'mugon' } else { $fallback }
& $mugonPath editor install
if ($LASTEXITCODE -ne 0) { throw "mugon editor install failed" }

Write-Host "Next: run 'mugon editor --help'. The scene editor and thumbnail renderer need a Vulkan/DX12-capable GPU driver." -ForegroundColor Green
