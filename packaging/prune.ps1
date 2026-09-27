<#
Remove what an end user's machine will never read. The Windows counterpart of
prune.sh, and the same reasoning: a shipped tree carries static archives left
over from linking, the .gem files RubyGems keeps after installing, headers for
compiling extensions that are already compiled, and each gem's own test suite.

One trap worth repeating from the Unix version: deleting every directory named
`test` also removes rack-test's lib\rack\test, which is library code, and the
app then fails to boot. Only each gem's root is touched.
#>
param([Parameter(Mandatory = $true)][string]$Root)
$ErrorActionPreference = "Continue"

function Size($path) {
  if (-not (Test-Path $path)) { return 0 }
  (Get-ChildItem $path -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
}

$before = Size $Root

Get-ChildItem $Root -Recurse -File -Include *.a, *.gem -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
if (Test-Path "$Root\ruby\include") { Remove-Item -Recurse -Force "$Root\ruby\include" }

foreach ($base in @("$Root\gems\gems", "$Root\ruby\lib\ruby\gems\3.4.0\gems")) {
  if (-not (Test-Path $base)) { continue }
  Get-ChildItem $base -Directory -ErrorAction SilentlyContinue | ForEach-Object {
    foreach ($unwanted in @("test", "spec", "features")) {
      $path = Join-Path $_.FullName $unwanted
      if (Test-Path $path) { Remove-Item -Recurse -Force $path -ErrorAction SilentlyContinue }
    }
  }
}

$after = Size $Root
"  {0,-28} {1:N0} MB -> {2:N0} MB  (saved {3:N0} MB)" -f "total", ($before / 1MB), ($after / 1MB), (($before - $after) / 1MB) | Write-Host
