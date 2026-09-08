# Registers hummingbird-open: for the current user (ADR-0036). No admin needed:
# HKCU\Software\Classes is per-user. Re-run after moving the checkout - the
# command line below carries open.ps1's absolute path.

$ErrorActionPreference = "Stop"
$handler = Join-Path $PSScriptRoot "open.ps1"
$key = "HKCU:\Software\Classes\hummingbird-open"

New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name "(Default)" -Value "URL:Hummingbird open"
Set-ItemProperty -Path $key -Name "URL Protocol" -Value ""
New-Item -Path "$key\shell\open\command" -Force | Out-Null
$command = "powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$handler`" `"%1`""
Set-ItemProperty -Path "$key\shell\open\command" -Name "(Default)" -Value $command

Write-Host "Registered hummingbird-open: -> $handler"
Write-Host "Try: Start-Process 'hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf'"
