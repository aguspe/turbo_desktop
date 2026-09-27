<#
Fetch RubyInstaller's portable archive and check it relocates.

Windows is fetched rather than built. RubyInstaller already ships a portable
interpreter that resolves relative to itself, so building a second one would be
work for its own sake. Spike 3 established that it relocates, tolerates a path
containing a space, and boots Rails with no compiler present.
#>
param(
  [string]$Version = "3.4.10",
  [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = "Stop"

$url = "https://github.com/oneclick/rubyinstaller2/releases/download/RubyInstaller-$Version-1/rubyinstaller-$Version-1-x64.7z"
Write-Host "Fetching $url"
curl.exe -fsSL -o ruby.7z $url
7z x ruby.7z -oC:\unpacked | Out-Null
$src = (Get-ChildItem C:\unpacked | Select-Object -First 1).FullName

New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
Copy-Item -Recurse $src $Out

& "$Out\bin\ruby.exe" -e @"
require 'psych'
require 'openssl'
abort 'psych broken' unless Psych.load('- 1') == [1]
abort 'openssl mismatch' unless OpenSSL::OPENSSL_VERSION == OpenSSL::OPENSSL_LIBRARY_VERSION
abort 'RbConfig did not follow the binary' unless RbConfig::CONFIG['prefix'].include?('out')
puts "OK  ruby #{RUBY_VERSION} #{RUBY_PLATFORM}, psych #{Psych::VERSION}, #{OpenSSL::OPENSSL_VERSION}"
"@
if ($LASTEXITCODE -ne 0) { throw "the portable interpreter did not check out" }
