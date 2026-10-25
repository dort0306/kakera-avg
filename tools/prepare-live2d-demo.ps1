param([Parameter(Mandatory=$true)][string]$Sdk, [string]$Output='target/local-live2d-game')
$ErrorActionPreference='Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (Test-Path -LiteralPath $Output) { throw "Output already exists: $Output. Choose a new directory." }
    $sdkPath=(Resolve-Path -LiteralPath $Sdk).Path
    $dll=Join-Path $sdkPath 'Core/dll/windows/x86_64/Live2DCubismCore.dll'
    $model=Join-Path $sdkPath 'Samples/Resources/Haru'
    if (-not (Test-Path -LiteralPath $dll) -or -not (Test-Path -LiteralPath $model)) { throw 'SDK must contain Windows x64 Core and Haru sample.' }
    New-Item -ItemType Directory -Path $Output | Out-Null
    Copy-Item -LiteralPath games/demo/story.xml,games/demo/ui.xml,games/demo/assets,games/demo/scripts,games/demo/chapters -Destination $Output -Recurse
    # The base demo already registers dedicated Live2D scenes. Refresh assets only.
    $destination=Join-Path $Output 'assets/live2d/Haru'
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Get-ChildItem -LiteralPath $model | Copy-Item -Destination $destination -Recurse -Force
    Copy-Item -LiteralPath $dll -Destination $Output
    Copy-Item -LiteralPath (Join-Path $sdkPath 'LICENSE.md') -Destination (Join-Path $Output 'LIVE2D-SDK-LICENSE.md')
    Copy-Item -LiteralPath (Join-Path $sdkPath 'Core/LICENSE.md') -Destination (Join-Path $Output 'LIVE2D-CORE-LICENSE.md')
    Copy-Item -LiteralPath (Join-Path $sdkPath 'Core/RedistributableFiles.txt') -Destination $Output
    Write-Output "Local Live2D game: $((Resolve-Path -LiteralPath $Output).Path)"
    Write-Output 'This generated SDK/sample game is local test content; review included Live2D terms before distribution.'
} finally { Pop-Location }
