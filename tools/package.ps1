param(
    [string]$Output = 'target/package/kakera-avg-v0.9.1',
    [string]$Project = 'games/demo',
    [ValidateRange(1,22)][int]$CompressionLevel = 19,
    [ValidateSet(1,2)][int]$Format = 2
)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (Test-Path -LiteralPath $Output) { throw "Output already exists: $Output. Choose a new -Output directory." }
    cargo build --locked --release --bins
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    New-Item -ItemType Directory -Path $Output | Out-Null
    # UI includes/ui-media and every declared story entry are preserved; player checkpoints are excluded.
    & ./target/release/kakera-pack.exe $Project (Join-Path $Output 'game.kpak') --level $CompressionLevel --format $Format
    if ($LASTEXITCODE -ne 0) { throw 'Resource validation or packing failed; output directory retained for diagnosis.' }
    Copy-Item -LiteralPath target/release/kakera-avg.exe -Destination $Output
    Copy-Item -LiteralPath runtime/live2d/windows-x64/Live2DCubismCore.dll -Destination $Output
    Copy-Item -LiteralPath licenses -Destination (Join-Path $Output 'licenses') -Recurse
    Copy-Item -LiteralPath runtime/live2d/LIVE2D-CORE-LICENSE.md,runtime/live2d/RedistributableFiles.txt -Destination (Join-Path $Output 'licenses')
    $gameLicenses=Join-Path $Project 'assets/licenses'
    if (Test-Path -LiteralPath $gameLicenses) { Copy-Item -LiteralPath $gameLicenses -Destination (Join-Path $Output 'licenses/game') -Recurse }
    Copy-Item -LiteralPath README.md -Destination $Output
    New-Item -ItemType Directory -Path (Join-Path $Output 'doc') | Out-Null
    Get-ChildItem -LiteralPath doc -File | Where-Object { $_.Extension -in '.md','.json' } | Copy-Item -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/archive -Destination (Join-Path $Output 'doc/archive') -Recurse
    if (Test-Path -LiteralPath doc/editor) {
        Copy-Item -LiteralPath doc/editor -Destination (Join-Path $Output 'doc/editor') -Recurse
    }
    Write-Output "Release package: $((Resolve-Path -LiteralPath $Output).Path)"
} finally { Pop-Location }
