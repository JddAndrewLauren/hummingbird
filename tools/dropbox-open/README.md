# `hummingbird-open:` — the once-per-machine helper (ADR-0036)

The web app cannot open a local file: a page served over https is refused
`file://` by every browser, no OS ships a URL scheme that opens Explorer or
Finder at a path, and Dropbox's desktop app publishes none either. So the
app fires a scheme this repo owns —

```
hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf
```

— and this helper, installed once per machine, claims that scheme, maps the
Dropbox-relative path onto **this machine's** Dropbox folder, and opens the
result in its default app (`Start-Process` / `open`). A folder path opens
Explorer or Finder at that folder. The machine-local root lives here and
nowhere else: the app stores only the relative path (`file_links.path`).

The scheme is spelled with no `//` on purpose (`client/web/src/dropbox/file-link.ts`'s
header says why). Renaming it is a three-place change: that constant and both
registrations below.

## What both helpers refuse

Before joining the path to the root, both refuse: an empty path, a leading
`/` or `\`, any `..` segment, a drive letter, a leading `~` — and, after
joining, any result that does not resolve to somewhere under the root. A
refused path is reported in a message box and nothing is opened. The app's
own `isValidFilePath` applies the same rules before it ever fires the
scheme; the helper repeats them because a URL can come from anywhere.

## What both helpers reveal instead of opening

An executable file type is never opened, only shown: Windows runs
`explorer.exe /select,<target>` and the Mac runs `open -R <target>` for a
resolved path whose extension is `.exe .bat .cmd .com .ps1 .vbs .js .msi
.scr .lnk` (Windows) or `.app .command .sh .pkg .dmg .scpt .workflow` (Mac;
a `.app` bundle is caught by name, not by being a directory). The decision
is by name alone, so a `.pdf` is opened whatever it contains. This is
ADR-0036 decision 5 as amended in the wrap-up of 2026-09-08. `test.sh`'s
`--dry-run` prints such a path as `reveal: <path>`.

## Not in CI, by design

There is no artefact to build and no host to test on: a URL-scheme handler
is a per-machine registration against a live desktop session. Each half
carries a table-driven test of its pure resolver (`windows/open.tests.ps1`,
`mac/test.sh`) that is run by hand on that machine.

## Windows

```powershell
cd tools\dropbox-open\windows
.\install.ps1              # registers HKCU\Software\Classes\hummingbird-open
.\open.tests.ps1           # the resolver's table-driven checks
Start-Process "hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf"
```

The root defaults to `C:\Dropbox`. To change it, put a `config.json` beside
`open.ps1`: `{ "root": "D:\\Dropbox" }`. `uninstall.ps1` removes the key.

The handler is PowerShell end to end. Do **not** route `%1` through a `.cmd`
— the URL is full of `%2F`, and batch `%`-expansion eats exactly that.

On first click Chrome or Edge asks "Open Hummingbird Open?"; tick "Always
allow" for the app's origin.

## Mac

```sh
cd tools/dropbox-open/mac
bash build.sh              # builds "Hummingbird Open.app" and registers it
bash test.sh               # the resolver's table-driven checks
open "hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf"
```

The root defaults to `$HOME/Library/CloudStorage/Dropbox`. To change it,
write the folder's path into `~/.config/hummingbird/dropbox-root`. The app
bundle is an AppleScript applet (`osacompile`) whose Info.plist claims the
scheme; it hands the URL to `resolve.sh`, which does the work. Re-run
`build.sh` after moving the checkout — the applet calls `resolve.sh` by
absolute path.

## Not installed?

The item panel always draws an "on dropbox.com" link beside Open. It opens
the same path in Dropbox's web UI and needs nothing installed.
