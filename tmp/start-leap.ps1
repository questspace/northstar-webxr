$result = 'C:\Users\takac\vibestar\tmp\svc-start.txt'
try {
    Start-Service -Name 'UltraleapTracking' -ErrorAction Stop
    Start-Sleep -Seconds 2
    $svc = Get-Service -Name 'UltraleapTracking'
    "start-ok: $($svc.Status)" | Out-File -FilePath $result -Encoding utf8
} catch {
    "start-failed: $($_.Exception.Message)" | Out-File -FilePath $result -Encoding utf8
}
