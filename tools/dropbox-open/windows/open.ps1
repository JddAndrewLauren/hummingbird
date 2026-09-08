# hummingbird-open: handler (ADR-0036). Maps a Dropbox-relative path onto this
# machine's Dropbox folder and opens it. See ../README.md.
#
# Dot-source this file to get Resolve-HummingbirdPath and Get-HummingbirdAction
# without running the handler (open.tests.ps1 does): the handler body only
# runs when a URL is passed as the first argument.

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

# Executable types are revealed in Explorer, never opened (ADR-0036 decision 5
# as amended 2026-09-08). Returns "open" or "reveal", by extension alone.
function Get-HummingbirdAction {
    param([string] $Path)
    $reveal = @(".exe", ".bat", ".cmd", ".com", ".ps1", ".vbs", ".js", ".msi", ".scr", ".lnk")
    if ($reveal -contains [IO.Path]::GetExtension($Path).ToLowerInvariant()) { return "reveal" }
    return "open"
}

function Get-HummingbirdRoot {
    $configPath = Join-Path $PSScriptRoot "config.json"
    if (Test-Path -LiteralPath $configPath) {
        $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
        if ($config.root) {
            $root = [string] $config.root
            # A relative root is relative to this script, not to the process
            # cwd (System32 when launched from a URL).
            if (-not [IO.Path]::IsPathRooted($root)) { $root = Join-Path $PSScriptRoot $root }
            return $root
        }
    }
    return "C:\Dropbox"
}

if ($Url) {
    try {
        $target = Resolve-HummingbirdPath -Url $Url -Root (Get-HummingbirdRoot)
        if (-not (Test-Path -LiteralPath $target)) { throw "not found: $target" }
        if ((Get-HummingbirdAction $target) -eq "reveal") {
            Start-Process -FilePath "explorer.exe" -ArgumentList "/select,`"$target`""
        } else {
            Start-Process -FilePath $target
        }
    } catch {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show("hummingbird could not open that file link.`n`n$($_.Exception.Message)", "hummingbird") | Out-Null
        exit 1
    }
}
