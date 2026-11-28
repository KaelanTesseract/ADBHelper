# Downloads the hero portraits from github.com/arnecls/afjk-characters into
# target/portraits for the installer. Files are named by the hero key
# ("Elijah & Lailah.png" -> "elijahlailah.png"), as ADBHelper looks them up.
# The pictures are not kept in this repository.
param([string]$Destination = 'target/portraits')

$ErrorActionPreference = 'Stop'
$listing = 'https://api.github.com/repos/arnecls/afjk-characters/contents/site/assets/portraits'
$raw = 'https://raw.githubusercontent.com/arnecls/afjk-characters/main/site/assets/portraits/'
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
$headers = @{ 'User-Agent' = 'ADBHelper-release-build' }
if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
$files = Invoke-RestMethod -Uri $listing -Headers $headers |
    Where-Object { $_.type -eq 'file' -and $_.name -like '*.png' }
$count = 0
foreach ($file in $files) {
    $key = ($file.name -replace '\.png$', '').ToLowerInvariant() -replace '[^a-z0-9]', ''
    $target = Join-Path $Destination "$key.png"
    $url = $raw + [uri]::EscapeDataString($file.name)
    Invoke-WebRequest -Uri $url -Headers $headers -OutFile $target -UseBasicParsing
    $count++
}
if ($count -lt 100) { throw "Only $count hero portraits were downloaded." }
Write-Host "$count hero portraits in $Destination"
