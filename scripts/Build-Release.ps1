param([string]$Version = '')
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$packageVersion = ((Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value)
if ($Version -and $Version.TrimStart('v') -ne $packageVersion) { throw 'Tag and Cargo version differ.' }
$notes = Join-Path 'releases' "v$packageVersion.md"
if (!(Test-Path -LiteralPath $notes) -or (Get-Content -LiteralPath $notes -Raw).Trim().Length -lt 40) { throw 'Write release patchnotes before publishing.' }
cargo test --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
New-Item -ItemType Directory -Path 'dist' -Force | Out-Null
$asset = Join-Path 'dist' 'ADBHelper-Windows-x64.exe'
Copy-Item -LiteralPath 'target/release/adbhelper.exe' -Destination $asset -Force
$hash = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$asset.sha256" -Value "$hash  ADBHelper-Windows-x64.exe" -Encoding ascii
Write-Output "Release v$packageVersion prepared; patchnotes: $notes"
