param([string]$Output = 'target/localization-demo')
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (Test-Path -LiteralPath $Output) { throw "Output already exists: $Output" }
    New-Item -ItemType Directory -Path (Join-Path $Output 'assets/licenses') | Out-Null
    Copy-Item -Path 'doc/localization-example/*' -Destination $Output
    Copy-Item -LiteralPath 'games/demo/assets/fonts/chinese.otf' -Destination (Join-Path $Output 'assets/chinese.otf')
    Copy-Item -LiteralPath 'games/demo/assets/audio/lin.wav','games/demo/assets/audio/knock.wav' -Destination (Join-Path $Output 'assets')
    Copy-Item -Path 'games/demo/assets/licenses/*' -Destination (Join-Path $Output 'assets/licenses') -Recurse
    Write-Output "Prepared localization demo: $((Resolve-Path -LiteralPath $Output).Path)"
} finally { Pop-Location }
