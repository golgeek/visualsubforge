# Installation requires network access for ordinary desktop runtimes. The
# subsequent packaged CLI check runs with --network none. No compiler, FFmpeg
# package or media SDK is installed separately here.
FROM ubuntu:24.04
COPY preview.deb /tmp/preview.deb
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends /tmp/preview.deb python3 \
    && rm /tmp/preview.deb \
    && ldd /usr/lib/visualsubforge/visualsubforge > /tmp/gui-libraries.txt \
    && ! grep -q 'not found' /tmp/gui-libraries.txt
