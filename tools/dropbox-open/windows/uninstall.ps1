# Removes the hummingbird-open: registration for the current user.
$ErrorActionPreference = "Stop"
Remove-Item -Path "HKCU:\Software\Classes\hummingbird-open" -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "Removed hummingbird-open:"
