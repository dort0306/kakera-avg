param(
    [string]$AssetRoot = (Join-Path $PSScriptRoot '../games/demo/assets')
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$ui = Join-Path $AssetRoot 'ui'
New-Item -ItemType Directory -Force $ui | Out-Null

# Geometric particle textures authored for this demo. No external artwork is used.
function New-Canvas([int]$width, [int]$height) {
    $bitmap = [System.Drawing.Bitmap]::new($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([System.Drawing.Color]::Transparent)
    return @{ Bitmap = $bitmap; Graphics = $graphics }
}

# Rain: a soft vertical streak that fades at both ends.
$rain = New-Canvas 6 36
$rainGradient = [System.Drawing.Drawing2D.LinearGradientBrush]::new(
    [System.Drawing.Point]::new(0, 0), [System.Drawing.Point]::new(0, 36),
    [System.Drawing.Color]::FromArgb(0, 198, 226, 245),
    [System.Drawing.Color]::FromArgb(220, 198, 226, 245))
$rainBlend = [System.Drawing.Drawing2D.ColorBlend]::new(3)
$rainBlend.Colors = [System.Drawing.Color[]]@(
    [System.Drawing.Color]::FromArgb(0, 198, 226, 245),
    [System.Drawing.Color]::FromArgb(220, 224, 240, 255),
    [System.Drawing.Color]::FromArgb(30, 198, 226, 245))
$rainBlend.Positions = [float[]]@(0.0, 0.45, 1.0)
$rainGradient.InterpolationColors = $rainBlend
$rain.Graphics.FillRectangle($rainGradient, 0, 0, 6, 36)
$rain.Bitmap.Save((Join-Path $ui 'particle-rain.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$rainGradient.Dispose(); $rain.Graphics.Dispose(); $rain.Bitmap.Dispose()

# Snow: a soft white dot built from three fading rings.
$snow = New-Canvas 16 16
foreach ($ring in @(@(0, 0, 16, 16, 60), @(2, 2, 12, 12, 140), @(5, 5, 6, 6, 255))) {
    $brush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb($ring[4], 255, 255, 255))
    $snow.Graphics.FillEllipse($brush, $ring[0], $ring[1], $ring[2], $ring[3])
    $brush.Dispose()
}
$snow.Bitmap.Save((Join-Path $ui 'particle-snow.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$snow.Graphics.Dispose(); $snow.Bitmap.Dispose()

# Petal: a pink oval with a lighter highlight.
$petal = New-Canvas 18 18
$petalBody = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(235, 244, 168, 196))
$petal.Graphics.FillEllipse($petalBody, 1, 3, 16, 12)
$petalLight = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(180, 255, 232, 240))
$petal.Graphics.FillEllipse($petalLight, 4, 5, 9, 6)
$petal.Bitmap.Save((Join-Path $ui 'particle-petal.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$petal.Graphics.Dispose(); $petal.Bitmap.Dispose(); $petalBody.Dispose(); $petalLight.Dispose()

# Light mote: a four point star with a bright core.
$spark = New-Canvas 20 20
$sparkGlow = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(70, 255, 246, 214))
$spark.Graphics.FillEllipse($sparkGlow, 0, 0, 20, 20)
$sparkPoints = [System.Drawing.Point[]]@(
    [System.Drawing.Point]::new(10, 0), [System.Drawing.Point]::new(13, 7),
    [System.Drawing.Point]::new(20, 10), [System.Drawing.Point]::new(13, 13),
    [System.Drawing.Point]::new(10, 20), [System.Drawing.Point]::new(7, 13),
    [System.Drawing.Point]::new(0, 10), [System.Drawing.Point]::new(7, 7))
$sparkBrush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(230, 255, 250, 232))
$spark.Graphics.FillPolygon($sparkBrush, $sparkPoints)
$sparkCore = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$spark.Graphics.FillEllipse($sparkCore, 7, 7, 6, 6)
$spark.Bitmap.Save((Join-Path $ui 'particle-spark.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$spark.Graphics.Dispose(); $spark.Bitmap.Dispose(); $sparkGlow.Dispose(); $sparkBrush.Dispose(); $sparkCore.Dispose()

Write-Output 'Generated particle-rain.png, particle-snow.png, particle-petal.png and particle-spark.png.'
