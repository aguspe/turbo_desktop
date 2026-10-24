<#
Turn a Rails app plus a relocatable interpreter into a self-contained Windows
application directory, and a zip of it.

The same contract as the macOS and Linux packers, because the parts that matter
are not platform-specific: the interpreter must relocate, `rails server` must
not be used, writable state lives outside the application tree, and the server
announces itself on stdout and exits when stdin closes.

What differs is the shape. There is no bundle format and no code signing here,
and writable state goes to %LOCALAPPDATA% rather than Application Support or
XDG_DATA_HOME.

Two things this deliberately does not do:

  * It does not build an installer. Tauri's bundler already produces the MSI and
    NSIS packages, and duplicating that would be work for its own sake.
  * The launcher is a .cmd, which means a console window. A shipped app wants a
    GUI shim instead; the Tauri shell is that shim, and this directory is what
    it runs.

Usage:
  packaging\pack-windows.ps1 -App ..\my_rails_app -Runtime out\ruby `
    -Gems out\gems -Name "Ledger" -AppId dev.example.ledger
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$App,
  [Parameter(Mandatory = $true)][string]$Runtime,
  [string]$Gems = "",
  [string]$Name = "Turbo Desktop App",
  [string]$AppId = "dev.turbodesktop.app",
  [string]$Out = "$PWD\dist"
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path

if (-not (Test-Path (Join-Path $App "config.ru"))) { throw "$App has no config.ru - is it a Rails app?" }
if (-not (Test-Path (Join-Path $Runtime "bin\ruby.exe"))) { throw "$Runtime has no bin\ruby.exe" }

function Step($message) { Write-Host "`n==> $message" -ForegroundColor White }

$slug = ($Name -replace '\s+', '-').ToLower()
$dir = Join-Path $Out $slug

Step "Assembling $slug"
if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
New-Item -ItemType Directory -Force -Path "$dir\lib" | Out-Null
Copy-Item -Recurse $Runtime "$dir\lib\ruby"
if ($Gems -and (Test-Path $Gems)) { Copy-Item -Recurse $Gems "$dir\lib\gems" }

# robocopy rather than Copy-Item: it handles long paths, which a deep
# vendor/bundle tree reaches quickly on Windows.
$null = robocopy $App "$dir\lib\app" /E /NFL /NDL /NJH /NJS /NP /XD tmp log .git node_modules
if ($LASTEXITCODE -ge 8) { throw "copying the app failed (robocopy $LASTEXITCODE)" }
$global:LASTEXITCODE = 0

Copy-Item "$here\templates\boot.rb" "$dir\lib\app\boot.rb" -Force
Write-Host "  interpreter, gems and app copied"

@"
@echo off
rem Resolve the interpreter beside this script, wherever the tree was unpacked,
rem and keep everything writable outside it. The tree may sit somewhere
rem read-only such as C:\Program Files.
setlocal
set "HERE=%~dp0"
if "%DESKTOP_DATA_DIR%"=="" set "DESKTOP_DATA_DIR=%LOCALAPPDATA%\$AppId"
if not exist "%DESKTOP_DATA_DIR%\tmp" mkdir "%DESKTOP_DATA_DIR%\tmp"
if not exist "%DESKTOP_DATA_DIR%\log" mkdir "%DESKTOP_DATA_DIR%\log"
if not exist "%DESKTOP_DATA_DIR%\storage" mkdir "%DESKTOP_DATA_DIR%\storage"

rem Gems may sit flat (GEM_HOME) or nested under ruby\<abi> (BUNDLE_PATH).
rem Accept both rather than depending on how they were installed.
set "GEMS=%HERE%lib\gems"
if exist "%GEMS%\ruby" for /d %%D in ("%GEMS%\ruby\*") do set "GEMS=%%~fD"
set "GEM_HOME=%GEMS%"
set "GEM_PATH=%GEMS%;%HERE%lib\ruby\lib\ruby\gems\3.4.0"
if "%RAILS_ENV%"=="" set "RAILS_ENV=production"
set "BUNDLE_GEMFILE=%HERE%lib\app\Gemfile"

cd /d "%HERE%lib\app"
rem Never ``rails server``: railties creates tmp dirs under Rails.root
rem regardless of config.paths, which fails when the tree is read-only.
if "%~1"=="" (
  "%HERE%lib\ruby\bin\ruby.exe" boot.rb
) else (
  "%HERE%lib\ruby\bin\ruby.exe" %*
)
"@ | Set-Content -Encoding ASCII "$dir\$slug.cmd"

Step "Pruning"
& "$here\prune.ps1" "$dir\lib"

Step "Packaging"
$zip = Join-Path $Out "$slug-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $dir -DestinationPath $zip
$size = "{0:N0} MB" -f ((Get-Item $zip).Length / 1MB)
Write-Host "  $zip ($size)"

Step "Result"
$total = "{0:N0} MB" -f ((Get-ChildItem $dir -Recurse -File | Measure-Object Length -Sum).Sum / 1MB)
Write-Host "  $dir ($total)"
