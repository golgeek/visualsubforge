# A virtual WASAPI endpoint for disposable hosted runners, never an app dependency.
param([Parameter(Mandatory = $true)][string]$Output)

$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Virtual audio setup is restricted to disposable GitHub-hosted Windows runners'
}
if (Test-Path $Output) { throw "Refusing to reuse audio evidence directory: $Output" }
New-Item -ItemType Directory -Path $Output | Out-Null
$Output = (Resolve-Path $Output).Path
$archive = Join-Path $Output 'Scream3.6.zip'
$source = Join-Path $Output 'unpacked'
$receipt = [ordered]@{
    status = 'incomplete'
    scope = 'CI-only virtual WASAPI endpoint; no physical audio or device-clock acceptance'
    url = 'https://github.com/duncanthrax/scream/releases/download/3.6/Scream3.6.zip'
    sha256 = '25ea5e778b4e6995a98d448b9b5f6d321f681663f1aeeec69d8e63183d008b19'
    source = 'https://github.com/duncanthrax/scream/tree/2d390fa7a916fc04fcc65d245b6f594ae1344d7b'
    license = 'MS-PL'
    destination = '127.0.0.1:4010'
    devices_before = @(Get-CimInstance Win32_SoundDevice | Select-Object Name, Status, PNPDeviceID)
    files = [ordered]@{}
}
try {
    Invoke-WebRequest $receipt.url -OutFile $archive -MaximumRetryCount 3 -RetryIntervalSec 2
    if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $receipt.sha256) {
        throw 'Scream release archive differs from the reviewed checksum'
    }
    Expand-Archive $archive $source
    $driver = Join-Path $source 'Install/driver/Scream.sys'
    $signature = Get-AuthenticodeSignature $driver
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate) {
        throw "Virtual audio driver signature is not valid: $($signature.Status)"
    }
    $receipt.signer = [ordered]@{
        subject = $signature.SignerCertificate.Subject
        thumbprint = $signature.SignerCertificate.Thumbprint
        status = [string]$signature.Status
    }
    foreach ($relative in @('Install/driver/Scream.sys', 'Install/driver/scream.cat', 'Install/driver/Scream.inf', 'Install/helpers/devcon.exe')) {
        $receipt.files[$relative] = (Get-FileHash (Join-Path $source $relative) -Algorithm SHA256).Hash.ToLowerInvariant()
    }

    # Trust this verified publisher only in the temporary VM. Keep ordinary
    # kernel signature enforcement; do not enable test signing or change roots.
    $store = [System.Security.Cryptography.X509Certificates.X509Store]::new('TrustedPublisher', 'LocalMachine')
    $store.Open('ReadWrite')
    try { $store.Add($signature.SignerCertificate) } finally { $store.Close() }
    Start-Service AudioEndpointBuilder
    Start-Service Audiosrv
    # The pinned driver's DriverEntry reads these options. Set them before its
    # first load so synthetic audio stays on loopback instead of multicast LAN.
    $options = 'HKLM:\SYSTEM\CurrentControlSet\Services\Scream\Options'
    New-Item $options -Force | Out-Null
    New-ItemProperty $options -Name UnicastIPv4 -Value '127.0.0.1' -PropertyType String -Force | Out-Null
    New-ItemProperty $options -Name UnicastPort -Value 4010 -PropertyType DWord -Force | Out-Null
    $devcon = Join-Path $source 'Install/helpers/devcon.exe'
    Push-Location (Join-Path $source 'Install/driver')
    try {
        & $devcon install Scream.inf '*Scream' 2>&1 | Tee-Object (Join-Path $Output 'install.log')
        $receipt.install_exit = $LASTEXITCODE
        if ($LASTEXITCODE -ne 0) { throw "Virtual audio installation failed or requires reboot: $LASTEXITCODE" }
    } finally { Pop-Location }
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $devices = @(Get-CimInstance Win32_SoundDevice | Select-Object Name, Status, PNPDeviceID)
        $ready = @($devices | Where-Object { $_.Name -like '*Scream*' -and $_.Status -eq 'OK' })
        if ($ready.Count -eq 0) { Start-Sleep -Milliseconds 250 }
    } while ($ready.Count -eq 0 -and [DateTime]::UtcNow -lt $deadline)
    $receipt.devices_after = $devices
    $receipt.services = @(Get-Service AudioEndpointBuilder, Audiosrv | Select-Object Name, @{Name='Status'; Expression={[string]$_.Status}})
    if ($ready.Count -ne 1) { throw 'Expected exactly one healthy virtual audio device' }
    $installed = Join-Path $env:SYSTEMROOT 'System32/drivers/Scream.sys'
    $receipt.installed_driver_sha256 = (Get-FileHash $installed -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($receipt.installed_driver_sha256 -ne $receipt.files['Install/driver/Scream.sys']) {
        throw 'Installed virtual audio driver differs from the verified archive'
    }
    $receipt.status = 'passed'
} finally {
    $receipt | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Output 'virtual-audio.json')
}
