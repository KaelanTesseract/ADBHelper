<#
  Signs a published GitHub release on the maintainer's PC and uploads the signatures.
  Run after the release workflow has published the files:

    ./scripts/Sign-Release.ps1 -Version v0.4.11

  Until this has run, the app does not offer the release as an update.
  Uploading changes the public release: run it only after the maintainer approved it.
  Needs: gh (signed in), the secret key from `cargo run --release --example release-signing -- keygen`.
#>
param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
if ($Version -notmatch '^v\d+\.\d+\.\d+$') { throw 'Use a release tag such as v0.4.11.' }
$files = @('ADBHelper-Windows-x64.exe', 'ADBHelper-Setup-x64.exe')
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("adbhelper-sign-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    foreach ($file in $files) {
        gh release download $Version --pattern $file --pattern "$file.sha256" --dir $work
        if ($LASTEXITCODE -ne 0) { throw "Could not download $file from release $Version." }
        $path = Join-Path $work $file
        $expected = ((Get-Content -LiteralPath "$path.sha256" -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($expected -ne $actual) { throw "$file does not match its published SHA-256. Do not sign it." }
        cargo run --release --locked --example release-signing -- sign $path $Version
        if ($LASTEXITCODE -ne 0) { throw "Signing $file failed." }
    }
    $signatures = $files | ForEach-Object { Join-Path $work "$_.minisig" }
    gh release upload $Version @signatures
    if ($LASTEXITCODE -ne 0) { throw 'Uploading the signatures failed.' }
    Write-Output "Release $Version signed. The app now offers it as an update."
}
finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
