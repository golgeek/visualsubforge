param([Parameter(Mandatory)][string]$Package, [Parameter(Mandatory)][string]$Installers)
$ErrorActionPreference = 'Stop'
$version = (Get-Content -Raw package.json | ConvertFrom-Json).version
$config = Get-Content -Raw src-tauri/tauri.conf.json | ConvertFrom-Json
$bundle = Join-Path $Package 'VisualSubForge'
$installer = @(Get-ChildItem $Installers -Filter '*.exe')
if ($installer.Count -ne 1) { throw 'Expected one NSIS installer' }
$files = @(Get-ChildItem $bundle -Recurse -Filter '*.exe') + $installer
$inventory = Get-Content -Raw (Join-Path $bundle 'native-inventory.json') | ConvertFrom-Json
$results = foreach ($file in $files) {
    $info = $file.VersionInfo
    if ($file.Name -in 'ffmpeg.exe','ffprobe.exe') {
        $upstream = @($inventory.files | Where-Object { $_.path -eq $file.Name -and $_.origin -eq 'ffmpeg' })
        if ($upstream.Count -ne 1 -or (Get-FileHash $file.FullName).Hash -ne $upstream[0].sha256) {
            throw "Unverified FFmpeg tool: $file"
        }
    } elseif ($info.ProductName -ne $config.productName -or $info.CompanyName -ne $config.bundle.publisher -or
              $info.LegalCopyright -ne $config.bundle.copyright -or $info.FileVersion -ne $version -or
              $info.ProductVersion -ne $version) {
        throw "Incorrect VSF version resource: $file, $info"
    }
    [ordered]@{file=$file.Name; product=$info.ProductName; version=$info.ProductVersion;
               fileVersion=$info.FileVersion; company=$info.CompanyName; copyright=$info.LegalCopyright}
}
$results | ConvertTo-Json | Set-Content (Join-Path $Package 'windows-metadata.json')
