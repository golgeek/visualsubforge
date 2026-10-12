# Extend the already installed candidate with virtual display/audio test tools.
# CLI installation checks run against the smaller original image separately.
FROM vsf-preview-install
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
       xvfb xauth dbus-x11 pulseaudio pulseaudio-utils mesa-utils libgl1-mesa-dri \
    && useradd --create-home --uid 10001 preview
USER preview
ENV GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
