# Installation requires network access for ordinary desktop runtimes. The
# subsequent packaged CLI check runs with --network none. No compiler, FFmpeg
# package or media SDK is installed separately here.
ARG BASE_IMAGE=ubuntu:24.04@sha256:9c7f200c07e3d4a03e7249d2c16bf66a51120fb876aa2242cf31451412543289
FROM ${BASE_IMAGE}
COPY preview.deb /tmp/preview.deb
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends /tmp/preview.deb python3 \
    && dpkg-deb -f /tmp/preview.deb Depends | tr ',' '\n' | awk '{print $1}' | xargs apt-cache policy > /tmp/dependency-policy.txt \
    && cat /tmp/dependency-policy.txt \
    && rm /tmp/preview.deb \
    && ldd /usr/lib/visualsubforge/visualsubforge > /tmp/gui-libraries.txt \
    && cat /tmp/gui-libraries.txt \
    && ! grep -q 'not found' /tmp/gui-libraries.txt
