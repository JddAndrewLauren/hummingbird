-- hummingbird-open: applet (ADR-0036). Compiled by build.sh, which also
-- writes resolve.sh's absolute path into Contents/Resources/resolve.path and
-- claims the scheme in the bundle's Info.plist. The applet does nothing but
-- hand the URL on.
on open location theURL
	set resolvePath to do shell script "cat " & quoted form of (POSIX path of (path to resource "resolve.path"))
	do shell script "/bin/bash " & quoted form of resolvePath & " " & quoted form of theURL
end open location
