# hummingbird-open: handler (ADR-0036). Maps a Dropbox-relative path onto this
# machine's Dropbox folder and opens it. See ../README.md.
#
# Dot-source this file to get Resolve-HummingbirdPath without running the
# handler (open.tests.ps1 does): the handler body only runs when a URL is
# passed as the first argument.

param([string] $Url)

$ErrorActionPreference = "Stop"
$Prefix = "hummingbird-open:?path="

function Resolve-HummingbirdPath {
    param([string] $Url, [string] $Root)

    if (-not $Url.StartsWith($Prefix)) { throw "not a hummingbird-open URL" }
    $encoded = $Url.Substring($Prefix.Length)
    $relative = [Uri]::UnescapeDataString($encoded).Trim()

    if ($relative -eq "") { throw "empty path" }
    if ($relative.StartsWith("/") -or $relative.StartsWith("\")) { throw "absolute path refused" }
    if ($relative.StartsWith("~")) { throw "home-relative path refused" }
    if ($relative -match '^[A-Za-z]:') { throw "drive letter refused" }
    foreach ($segment in ($relative -split '[/\\]')) {
        if ($segment -eq "..") { throw "'..' segment refused" }
    }

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    $joined = [IO.Path]::GetFullPath((Join-Path $rootFull ($relative -replace '/', '\')))
    if (-not $joined.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
        throw "path escapes the Dropbox root"
    }
    return $joined
}

function Get-HummingbirdRoot {
    $configPath = Join-Path $PSScriptRoot "config.json"
    if (Test-Path $configPath) {
        $config = Get-Content $configPath -Raw | ConvertFrom-Json
        if ($config.root) { return [string] $config.root }
    }
    return "C:\Dropbox"
}

if ($Url) {
    try {
        $target = Resolve-HummingbirdPath -Url $Url -Root (Get-HummingbirdRoot)
        if (-not (Test-Path $target)) { throw "not found: $target" }
        Start-Process -FilePath $target
    } catch {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show("hummingbird could not open that file link.`n`n$($_.Exception.Message)", "hummingbird") | Out-Null
        exit 1
    }
}
