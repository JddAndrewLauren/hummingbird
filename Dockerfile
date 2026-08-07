FROM python:3.12-slim

# supercronic pinned by version AND checksum. The sha256 is of the actual
# v0.2.33 linux-amd64 release asset (the project publishes no checksum file);
# re-verify with `shasum -a 256` after downloading if you bump the version.
ARG SUPERCRONIC_VERSION=v0.2.33
ARG SUPERCRONIC_SHA256=feefa310da569c81b99e1027b86b27b51e6ee9ab647747b49099645120cfc671

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl; \
    rm -rf /var/lib/apt/lists/*; \
    curl -fsSLo /usr/local/bin/supercronic \
      "https://github.com/aptible/supercronic/releases/download/${SUPERCRONIC_VERSION}/supercronic-linux-amd64"; \
    echo "${SUPERCRONIC_SHA256}  /usr/local/bin/supercronic" | sha256sum -c -; \
    chmod +x /usr/local/bin/supercronic

WORKDIR /app

# The repo file is sweep.py so tests can import it; the container wants a bare
# executable so `fly ssh console -C /app/sweep` works verbatim.
COPY sweep.py /app/sweep
COPY denylist.json /app/denylist.json
COPY crontab /app/crontab
RUN chmod +x /app/sweep

# Absolute path, not a bare `supercronic`: as PID 1 supercronic re-execs itself
# to reap dead processes, and that re-exec does not search PATH -- a bare
# argv[0] dies instantly with "Failed to fork exec: no such file or directory".
CMD ["/usr/local/bin/supercronic", "-passthrough-logs", "/app/crontab"]
