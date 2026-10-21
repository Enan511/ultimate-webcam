$ErrorActionPreference='Stop'
$adbPath=Join-Path $PSScriptRoot 'adb.exe'
$apkPath=Join-Path $PSScriptRoot 'Phone App/UWebcam.apk'
if(-not(Test-Path -LiteralPath $apkPath)){throw 'Phone App/UWebcam.apk is missing.'}
& $adbPath install -r $apkPath
if($LASTEXITCODE -ne 0){throw 'Installation failed. Connect one unlocked Android phone, enable USB debugging, and allow this computer.'}
Write-Host 'UWebcam installed. Open it on the phone and allow camera access.'
