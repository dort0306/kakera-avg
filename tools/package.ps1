param([string]$Output = 'target/package/kakera-avg')
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (Test-Path -LiteralPath $Output) {
        throw "Output already exists: $Output. Choose a new -Output directory."
    }
    cargo build --locked --release
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    New-Item -ItemType Directory -Path $Output | Out-Null
    Copy-Item -LiteralPath target/release/kakera-avg.exe -Destination $Output
    New-Item -ItemType Directory -Path (Join-Path $Output 'games') | Out-Null
    Copy-Item -LiteralPath games/demo -Destination (Join-Path $Output 'games/demo') -Recurse
    Copy-Item -LiteralPath readme.md -Destination $Output
    Write-Output "Portable build: $((Resolve-Path -LiteralPath $Output).Path)"
} finally {
    Pop-Location
}
