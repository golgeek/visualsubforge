param([int]$AppPid, [int]$Port, [string]$Evidence)
$ErrorActionPreference = 'Continue'
$log = Join-Path $Evidence "timeout-$AppPid.txt"
function Record($Value) {
    "$(Get-Date -Format o) $($Value | Out-String -Width 240)" | Out-File $log -Append -Encoding utf8
}

Record "Observed PID $AppPid port $Port"
Record (netstat -ano -p TCP | Select-String ":$Port\s")
$process = Get-Process -Id $AppPid -ErrorAction SilentlyContinue
if ($process) {
    Record ($process | Select-Object Id, Name, Responding, MainWindowTitle, MainWindowHandle, CPU, StartTime)
    Record ($process.Threads | Select-Object Id, ThreadState, WaitReason, TotalProcessorTime)
}
$processes = Get-CimInstance Win32_Process
Record ($processes | Where-Object { $_.ProcessId -eq $AppPid -or $_.ParentProcessId -eq $AppPid -or $_.Name -eq 'msedgewebview2.exe' } |
    Select-Object Name, ProcessId, ParentProcessId, ExecutablePath, CreationDate)
try {
    Add-Type -AssemblyName System.Windows.Forms,System.Drawing
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $image = New-Object Drawing.Bitmap $bounds.Width, $bounds.Height
    $graphics = [Drawing.Graphics]::FromImage($image)
    $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size)
    $image.Save((Join-Path $Evidence "timeout-$AppPid.png"), [Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $image.Dispose()
} catch { Record $_ }
# Use a non-invasive stack snapshot only if the runner has the Windows debugger.
$cdb = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Debuggers\x64\cdb.exe'
if ((Test-Path $cdb) -and $process) {
    & $cdb -pv -p $AppPid -c '~* k 16; q' 2>&1 | Out-File (Join-Path $Evidence "stacks-$AppPid.txt")
} else { Record 'cdb or the observed process unavailable' }
