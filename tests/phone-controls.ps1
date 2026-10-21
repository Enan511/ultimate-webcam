$ErrorActionPreference='Stop'
function New-Control {
    $client=[Net.Sockets.TcpClient]::new('127.0.0.1',27184)
    $client.GetStream().ReadTimeout=15000
    return $client
}
$control=New-Control
$writer=[IO.StreamWriter]::new($control.GetStream(),[Text.UTF8Encoding]::new($false))
$writer.AutoFlush=$true
$reader=[IO.StreamReader]::new($control.GetStream())
$checks=[Collections.Generic.List[string]]::new()
function Send-Control($body,[switch]$ExpectError) {
    $writer.WriteLine(($body|ConvertTo-Json -Compress))
    $line=$reader.ReadLine()
    if (-not $line) {throw 'Phone closed the control socket'}
    $reply=$line|ConvertFrom-Json
    if ($ExpectError) {
        if ($reply.status -ne 'ERROR') {throw 'Invalid mode should have been rejected'}
    } elseif ($reply.status -ne 'OK') {throw $reply.error}
    return $reply
}
try {
    $state=Send-Control @{cmd='GET_STATE'}
    if ($state.telemetry.isAudioActive) {throw 'Microphone should default to off'}
    $checks.Add('Microphone defaults off')
    $reply=Send-Control @{cmd='SET_AUTO_ORIENTATION';enabled=$false}
    if($reply.telemetry.autoOrientation -or $reply.telemetry.rotationDegrees -ne 0){throw 'Orientation lock did not disable automatic rotation'}
    $reply=Send-Control @{cmd='SET_AUTO_ORIENTATION';enabled=$true}
    if(-not $reply.telemetry.autoOrientation -or $reply.telemetry.rotationDegrees -notin @(0,90,180,270)){throw 'Automatic orientation state is invalid'}
    $checks.Add('Automatic orientation and manual lock')
    $reply=Send-Control @{cmd='SET_ZOOM';ratio=1.5}
    if ($reply.telemetry.zoomRatio -ne 1.5) {throw 'Zoom not synchronized'}
    $checks.Add('Hardware zoom')
    $reply=Send-Control @{cmd='SET_EXPOSURE';compensation=3}
    if ($reply.telemetry.exposureCompensation -ne 3) {throw 'Exposure not synchronized'}
    $checks.Add('Hardware exposure')
    $reply=Send-Control @{cmd='CONFIGURE';cameraId='1';width=1920;height=1080;fps=30;bitrate=15000000}
    if ($reply.telemetry.activeCameraId -ne '1' -or -not $reply.telemetry.isStreaming) {throw 'Front camera did not start'}
    $checks.Add('Front camera switch')
    $null=Send-Control @{cmd='CONFIGURE';cameraId='1';width=123;height=456;fps=30;bitrate=15000000} -ExpectError
    $null=Send-Control @{cmd='GET_STATE'}
    $checks.Add('Invalid mode rejected, connection retained')
    $reply=Send-Control @{cmd='CONFIGURE';cameraId='0';width=1280;height=720;fps=30;bitrate=8000000}
    if ($reply.telemetry.width -ne 1280 -or -not $reply.telemetry.isStreaming) {throw '720p mode did not start'}
    $checks.Add('Rear camera and resolution change')
    $reply=Send-Control @{cmd='STOP_STREAM'}
    if ($reply.telemetry.isStreaming) {throw 'Pause did not stop the camera'}
    $checks.Add('Pause')
    $reply=Send-Control @{cmd='START_STREAM'}
    if (-not $reply.telemetry.isStreaming) {throw 'Resume did not start the camera'}
    $checks.Add('Resume')
    $reply=Send-Control @{cmd='SET_PROFILE';profile='minimum'}
    if($reply.telemetry.width -notin @(320,640) -or $reply.telemetry.transport -ne 'jpeg'){throw 'Minimum-delay profile is invalid'}
    $reply=Send-Control @{cmd='SET_PROFILE';profile='1080'}
    if($reply.telemetry.width -ne 1920 -or $reply.telemetry.transport -ne 'h264'){throw 'Full-HD profile is invalid'}
    $null=Send-Control @{cmd='SET_PROFILE';profile='invalid'} -ExpectError
    $checks.Add('Both profiles and invalid-profile rejection')
    $null=Send-Control @{cmd='CONFIGURE';cameraId='0';width=1920;height=1080;fps=30;bitrate=15000000}
    $null=Send-Control @{cmd='SET_ZOOM';ratio=1.0}
    $null=Send-Control @{cmd='SET_EXPOSURE';compensation=0}
    $null=Send-Control @{cmd='SET_TORCH';enabled=$false}
    $checks.Add('Default configuration restored')
    $checks|ForEach-Object {Write-Output ('PASS: '+$_)}
    $checks|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $PSScriptRoot '..\artifacts\phone-control-tests.json')
} finally {$control.Dispose()}
