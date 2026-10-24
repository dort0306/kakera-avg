param([string]$Ffmpeg = 'ffmpeg', [switch]$Force)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    New-Item -ItemType Directory -Force -Path games/demo/assets/video | Out-Null
    foreach ($clip in @(@{Name='intro';Seconds=12;Hz=440},@{Name='credits';Seconds=8;Hz=660})) {
        $output = "games/demo/assets/video/$($clip.Name).mp4"
        if ((Test-Path -LiteralPath $output) -and -not $Force) { throw "Already exists: $output; use -Force to regenerate." }
        $fade = $clip.Seconds - 1
        $filter = "drawbox=x=0:y=540:w=iw:h=180:color=black@0.75:t=fill,drawtext=fontfile=games/demo/assets/fonts/chinese.otf:text='KAKERA 0.6 - $($clip.Name)':fontcolor=white:fontsize=56:x=(w-tw)/2:y=590"
        & $Ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'testsrc2=size=1280x720:rate=30' -f lavfi -i "sine=frequency=$($clip.Hz):sample_rate=48000" -t $clip.Seconds -vf $filter -af "volume=0.2,afade=t=in:d=0.3,afade=t=out:st=${fade}:d=1" -c:v libopenh264 -pix_fmt yuv420p -b:v 2500k -c:a aac -ac 2 -ar 48000 -movflags +faststart $output
        if ($LASTEXITCODE -ne 0) { throw "Video generation failed: $output" }
    }
} finally { Pop-Location }
