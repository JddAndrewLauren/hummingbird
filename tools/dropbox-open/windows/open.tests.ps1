# Table-driven checks for Resolve-HummingbirdPath and Get-HummingbirdAction.
# Run by hand:
#   .\open.tests.ps1
. (Join-Path $PSScriptRoot "open.ps1")

$root = "C:\Dropbox"
$failures = 0

function Check($url, $expected) {
    try {
        $actual = Resolve-HummingbirdPath -Url $url -Root $root
        if ($expected -eq $null) { Write-Host "FAIL $url -> expected refusal, got $actual"; $script:failures++ }
        elseif ($actual -ne $expected) { Write-Host "FAIL $url -> $actual, expected $expected"; $script:failures++ }
        else { Write-Host "ok   $url -> $actual" }
    } catch {
        if ($expected -eq $null) { Write-Host "ok   $url refused: $($_.Exception.Message)" }
        else { Write-Host "FAIL $url refused: $($_.Exception.Message)"; $script:failures++ }
    }
}

Check "hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf" "C:\Dropbox\Finance\2026\receipt.pdf"
Check "hummingbird-open:?path=House%2FTap%20washer.pdf"      "C:\Dropbox\House\Tap washer.pdf"
Check "hummingbird-open:?path=House%2FPlumbing"             "C:\Dropbox\House\Plumbing"
Check "hummingbird-open:?path="                             $null
Check "hummingbird-open:?path=%2Fetc%2Fpasswd"              $null
Check "hummingbird-open:?path=..%2F..%2FWindows"            $null
Check "hummingbird-open:?path=a%2F..%2Fb.pdf"               $null
Check "hummingbird-open:?path=D%3A%5Csecrets.txt"           $null
Check "hummingbird-open:?path=~%2Fx.pdf"                    $null
Check "not-ours:?path=x"                                    $null

function CheckAction($path, $expected) {
    $actual = Get-HummingbirdAction $path
    if ($actual -eq $expected) { Write-Host "ok   $path -> $actual" }
    else { Write-Host "FAIL $path -> $actual, expected $expected"; $script:failures++ }
}

CheckAction "C:\Dropbox\Finance\2026\receipt.pdf" "open"
CheckAction "C:\Dropbox\House\Plumbing"            "open"
CheckAction "C:\Dropbox\House\fix.ps1"             "reveal"
CheckAction "C:\Dropbox\House\Setup.EXE"           "reveal"
CheckAction "C:\Dropbox\House\shortcut.lnk"        "reveal"

if ($failures -gt 0) { Write-Host "$failures failure(s)"; exit 1 } else { Write-Host "all ok" }
