$result = 'C:\Users\takac\vibestar\tmp\svc-restart.txt'
try {
    Restart-Service -Name 'UltraleapTracking' -Force -ErrorAction Stop
    "restarted-ok" | Out-File -FilePath $result -Encoding utf8
} catch {
    "restart-failed: $($_.Exception.Message)" | Out-File -FilePath $result -Encoding utf8
}
