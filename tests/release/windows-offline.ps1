param(
    [Parameter(Mandatory)][string]$Output,
    [string]$Bundle,
    [string]$Fixtures,
    [switch]$ControlOnly
)

# Restrict this temporary machine-policy test to disposable GitHub Windows jobs.
# It blocks only the tested executables, leaving the runner's agent connected.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Run this isolation check only in a disposable GitHub Windows runner'
}
if (Test-Path -LiteralPath $Output) { throw 'Choose a new evidence directory' }
if (-not $ControlOnly -and (-not $Bundle -or -not $Fixtures)) {
    throw 'Bundle and Fixtures are required for package validation'
}
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path $Output | Out-Null
$python = (Get-Command python -CommandType Application).Source
$probe = Join-Path $PSScriptRoot 'windows-network-probe.py'
$cliOutput = Join-Path $Output 'cli'
$relocated = Join-Path $cliOutput 'Relocated VSF Preview'
$programs = @($python)
if (-not $ControlOnly) {
    $programs += @('vsf.exe', 'ffmpeg.exe', 'ffprobe.exe') | ForEach-Object { Join-Path $relocated $_ }
}
$profiles = @(Get-NetFirewallProfile -PolicyStore PersistentStore | Select-Object Name, Enabled)
$rules = @()
$cleanupErrors = @()
$failure = $null
$evidence = [ordered]@{
    status = 'incomplete'
    scope = 'Hosted Windows runner; executable-specific outbound isolation, not whole-runner disconnection'
    control_only = [bool]$ControlOnly
    programs = $programs
    original_profiles = @($profiles | ForEach-Object { @{ name = [string]$_.Name; enabled = [string]$_.Enabled } })
}

function Test-ConnectionControl {
    $json = & $python $probe $evidence.address
    if ($LASTEXITCODE -ne 0) { throw 'TCP control did not execute' }
    return $json | ConvertFrom-Json
}

try {
    if ((Get-Service MpsSvc).Status -ne 'Running') { throw 'Windows Firewall service is not running' }
    $evidence.address = [string]([Net.Dns]::GetHostAddresses('github.com') |
        Where-Object AddressFamily -eq InterNetwork | Select-Object -First 1)
    if (-not $evidence.address) { throw 'No IPv4 address for the connection control' }
    $evidence.before = Test-ConnectionControl
    if (-not $evidence.before.connected) { throw 'Connection control was already unreachable before isolation' }
    Set-NetFirewallProfile -Profile Domain, Private, Public -Enabled True
    $effective = @(Get-NetFirewallProfile -PolicyStore ActiveStore)
    if ($effective.Count -ne 3 -or @($effective | Where-Object Enabled -ne True).Count) {
        throw 'Effective firewall profiles are not all enabled'
    }
    $evidence.effective_profiles = @($effective | ForEach-Object { @{ name = [string]$_.Name; enabled = [string]$_.Enabled } })
    $evidence.active_rules = @()
    foreach ($program in $programs) {
        $name = 'VSF offline control ' + [guid]::NewGuid()
        # Record before creation so cleanup also covers a partially successful call.
        $rules += $name
        New-NetFirewallRule -Name $name -DisplayName $name -Profile Any -Enabled True `
            -Direction Outbound -Action Block -Program $program | Out-Null
        $active = Get-NetFirewallRule -PolicyStore ActiveStore -Name $name
        $filter = $active | Get-NetFirewallApplicationFilter
        if ($active.Enabled -ne 'True' -or $active.Action -ne 'Block' -or
            $active.Direction -ne 'Outbound' -or $active.Profile -ne 'Any' -or $filter.Program -ne $program) {
            throw "Effective program block differs from the requested rule: $name"
        }
        $evidence.active_rules += @{ name = $name; program = $filter.Program; action = [string]$active.Action }
    }
    $evidence.blocked = Test-ConnectionControl
    if ($evidence.blocked.connected) { throw 'Connection succeeded despite the program block' }
    if (-not $ControlOnly) {
        & $python (Join-Path $PSScriptRoot 'packaged-cli.py') --bundle $Bundle --bin . `
            --fixtures $Fixtures --output $cliOutput
        if ($LASTEXITCODE -ne 0) { throw 'Packaged CLI checks failed under network isolation' }
        $evidence.cli = 'cli/packaged-cli-check.json'
    }
} catch {
    $failure = $_.Exception.Message
} finally {
    # Restore each owned resource even when another cleanup operation fails.
    foreach ($name in $rules) {
        try {
            Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue | Remove-NetFirewallRule
            if (Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue) { throw "Rule remains: $name" }
        } catch { $cleanupErrors += $_.Exception.Message }
    }
    foreach ($profile in $profiles) {
        try {
            Set-NetFirewallProfile -Profile $profile.Name -Enabled $profile.Enabled
            $restored = Get-NetFirewallProfile -PolicyStore PersistentStore -Name $profile.Name
            if ($restored.Enabled -ne $profile.Enabled) { throw "Profile was not restored: $($profile.Name)" }
        } catch { $cleanupErrors += $_.Exception.Message }
    }
    if ($evidence.address) {
        try {
            $evidence.after = Test-ConnectionControl
            if (-not $evidence.after.connected) { throw 'Connection did not recover after removing isolation' }
        } catch { $cleanupErrors += $_.Exception.Message }
    }
    $evidence.cleanup_errors = $cleanupErrors
    $evidence.error = $failure
    $evidence.status = if (-not $failure -and -not $cleanupErrors.Count) { 'passed' } else { 'failed' }
    $evidence | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $Output 'network-isolation.json')
}
if ($evidence.status -ne 'passed') { throw "Offline check failed: $failure $($cleanupErrors -join '; ')" }
