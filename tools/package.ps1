param([string]$Output = 'target/package/kakera-avg-v0.6-live2d')
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
    Copy-Item -LiteralPath runtime/live2d/windows-x64/Live2DCubismCore.dll -Destination $Output
    New-Item -ItemType Directory -Path (Join-Path $Output 'games') | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $Output 'games/demo') | Out-Null
    Copy-Item -LiteralPath games/demo/story.xml,games/demo/ui.xml,games/demo/assets,games/demo/scripts -Destination (Join-Path $Output 'games/demo') -Recurse
    Copy-Item -LiteralPath licenses -Destination (Join-Path $Output 'licenses') -Recurse
    Copy-Item -LiteralPath runtime/live2d/LIVE2D-CORE-LICENSE.md,runtime/live2d/RedistributableFiles.txt -Destination (Join-Path $Output 'licenses')
    Copy-Item -LiteralPath readme.md -Destination $Output
    New-Item -ItemType Directory -Path (Join-Path $Output 'doc') | Out-Null
    Copy-Item -LiteralPath doc/rust-mvc-plan.zh-cn.md,doc/v0.2-implementation-plan.zh-cn.md,doc/v0.2-save.example.json,doc/v0.2-ui-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/v0.3-implementation-plan.zh-cn.md,doc/v0.3-save.example.json,doc/v0.3-ui-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/js-trigger-implementation-plan.zh-cn.md,doc/js-trigger-ui-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/v0.4-implementation-plan.zh-cn.md,doc/v0.4-ui-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/v0.5.1-portrait-plan.zh-cn.md,doc/v0.5.1-ui-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/xml-js-api.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/v0.6-implementation-plan.zh-cn.md,doc/v0.6-save.example.json,doc/v0.6-ui-validation.zh-cn.md,doc/live2d-demo-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/archive -Destination (Join-Path $Output 'doc/archive') -Recurse
    Copy-Item -LiteralPath doc/effects-implementation-plan.zh-cn.md,doc/effects-ui-validation.zh-cn.md -Destination (Join-Path $Output 'doc')
    Copy-Item -LiteralPath doc/rust-mvc-example,doc/v0.3-example,doc/v0.3.1-example -Destination (Join-Path $Output 'doc') -Recurse
    Write-Output "Portable build: $((Resolve-Path -LiteralPath $Output).Path)"
} finally {
    Pop-Location
}
