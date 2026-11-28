param([string]$Version = '')
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$packageVersion = ((Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value)
if ($Version -and $Version.TrimStart('v') -ne $packageVersion) { throw 'Tag and Cargo version differ.' }
$notes = Join-Path 'releases' "v$packageVersion.md"
if (!(Test-Path -LiteralPath $notes) -or (Get-Content -LiteralPath $notes -Raw).Trim().Length -lt 40) { throw 'Write release patchnotes before publishing.' }
if ((Get-Content -LiteralPath 'minisign.pub' -Raw) -match 'PLACEHOLDER') { throw 'minisign.pub is still the placeholder. Run: cargo run --release --example release-signing -- keygen' }
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw 'Code is not formatted. Run cargo fmt --all.' }
cargo clippy --release --locked --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Clippy found warnings.' }
cargo deny check
if ($LASTEXITCODE -ne 0) { throw 'cargo deny found a license, advisory, or source problem. See deny.toml.' }
cargo test --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
New-Item -ItemType Directory -Path 'dist' -Force | Out-Null
$asset = Join-Path 'dist' 'ADBHelper-Windows-x64.exe'
Copy-Item -LiteralPath 'target/release/adbhelper.exe' -Destination $asset -Force
$hash = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$asset.sha256" -Value "$hash  ADBHelper-Windows-x64.exe" -Encoding ascii

# Hero portraits for the Hero Box ship with the installer (game art from
# afjk-characters, see THIRD_PARTY.md); they are fetched here, not kept in git.
& ./scripts/Fetch-Portraits.ps1 -Destination 'target/portraits'

# Installer for new users (Inno Setup 6). The updater keeps using the plain EXE above.
$iscc = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
    (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
if (-not $iscc) { $iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source }
if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) not found. Install it from https://jrsoftware.org/isdl.php.' }
& $iscc /Q "/DAppVersion=$packageVersion" 'installer/adbhelper.iss'
if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
$setup = Join-Path 'dist' 'ADBHelper-Setup-x64.exe'
$setupHash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$setup.sha256" -Value "$setupHash  ADBHelper-Setup-x64.exe" -Encoding ascii
Write-Output "Release v$packageVersion prepared; patchnotes: $notes"
