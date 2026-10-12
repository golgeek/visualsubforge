# Fixture tooling only. Release packages build and test their own media runtime.
$ErrorActionPreference = 'Stop'
# Chocolatey can return success after a feed timeout and installing zero packages.
# Verify the executables, and retry only this external setup (never the tests).
$PSNativeCommandUseErrorActionPreference = $false
$bin = Join-Path $env:ChocolateyInstall 'lib/ffmpeg/tools/ffmpeg/bin'
for ($attempt = 1; $attempt -le 3; $attempt++) {
    choco install ffmpeg --version=9.0.2 --no-progress -y
    $installed = $LASTEXITCODE -eq 0
    foreach ($tool in @('ffmpeg', 'ffprobe')) {
        $executable = Join-Path $bin "$tool.exe"
        if (!(Test-Path $executable -PathType Leaf)) {
            $installed = $false
        } else {
            & $executable -version
            if ($LASTEXITCODE -ne 0) { $installed = $false }
        }
    }
    if ($installed) {
        $bin >> $env:GITHUB_PATH
        exit 0
    }
    if ($attempt -lt 3) { Start-Sleep -Seconds 10 }
}
throw 'FFmpeg fixture tools were not installed and executable after three attempts.'
