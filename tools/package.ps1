param([string]$Output = 'target/package/kakera-avg-v0.2')
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
    New-Item -ItemType Directory -Path (Join-Path $Output 'doc') | Out-Null
    Copy-Item -LiteralPath doc/rust-mvc-plan.zh-cn.md,doc/v0.2-implementation-plan.zh-cn.md,doc/v0.2-save.example.json,doc/v0.2-ui-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Write-Output "Portable build: $((Resolve-Path -LiteralPath $Output).Path)"
} finally {
    Pop-Location
}
