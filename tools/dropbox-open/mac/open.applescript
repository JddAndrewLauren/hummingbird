-- hummingbird-open: applet (ADR-0036). Compiled by build.sh, which also
-- rewrites RESOLVE_SH to resolve.sh's absolute path and claims the scheme in
-- the bundle's Info.plist. The applet does nothing but hand the URL on.
on open location theURL
	do shell script "/bin/bash " & quoted form of "RESOLVE_SH" & " " & quoted form of theURL
end open location
