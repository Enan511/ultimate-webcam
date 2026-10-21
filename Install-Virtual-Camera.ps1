param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'
$driverRoot = Join-Path $PSScriptRoot 'drivers\UnityCapture'
New-Item -ItemType Directory -Path (Join-Path $PSScriptRoot 'artifacts') -Force | Out-Null
$filters = @(
    @{ Name='UnityCaptureFilter64.dll'; Tool=Join-Path $env:SystemRoot 'System32\regsvr32.exe'; Hash='72812F5363D8ECB45632253F8C8C888844B1B62E27616F3C8CC21064CCDE25E5' },
    @{ Name='UnityCaptureFilter32.dll'; Tool=Join-Path $env:SystemRoot 'SysWOW64\regsvr32.exe'; Hash='AA3EBDF03DEA7F3AAB3DD7B724751F49ED71672256B57C6A19AA6809CABF30BA' }
)
foreach ($filter in $filters) {
    $dll = Join-Path $driverRoot $filter.Name
    if ((Get-FileHash -LiteralPath $dll -Algorithm SHA256).Hash -ne $filter.Hash) { throw "Filter verification failed: $dll" }
}
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    $arguments = '-NoProfile -File "' + $PSCommandPath + '"' + $(if ($Uninstall) {' -Uninstall'} else {''})
    $process = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -ArgumentList $arguments -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Setup was not completed. Please allow the Windows administrator prompt and try again.' }
    exit 0
}
foreach ($filter in $filters) {
    $dll = Join-Path $driverRoot $filter.Name
    $arguments = '/s ' + $(if ($Uninstall) { '/u ' } else { '' }) + '"' + $dll + '"'
    $process = Start-Process -FilePath $filter.Tool -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Registration failed ($($process.ExitCode)) for $($filter.Name)." }
}
if (-not $Uninstall) {
    foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)) {
        $hive=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine,$view)
        try {
            $cameraKey=$hive.OpenSubKey('SOFTWARE\Classes\CLSID\{860BB310-5D01-11D0-BD3B-00A0C911CE86}\Instance\{5C2CD55C-92AD-4999-8666-912BD3E70010}',$true)
            if($cameraKey){try{$cameraKey.SetValue('FriendlyName','Ultimate Webcam',[Microsoft.Win32.RegistryValueKind]::String)}finally{$cameraKey.Dispose()}}
        } finally {$hive.Dispose()}
    }
}
Set-Content -LiteralPath (Join-Path $PSScriptRoot 'artifacts\virtual-camera-setup.txt') -Value $(if ($Uninstall) {'Uninstalled'} else {'Installed'})
Write-Output $(if ($Uninstall) {'Virtual camera removed.'} else {'Ultimate Webcam is ready. Restart your meeting app to see it.'})
