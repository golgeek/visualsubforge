ARG BASE_IMAGE
FROM ${BASE_IMAGE}
ARG FORMAT
COPY candidate/ /tmp/package/
COPY tests/release/linux-matrix-install.sh /tmp/install.sh
RUN /bin/bash /tmp/install.sh "${FORMAT}"
ENV LIBGL_ALWAYS_SOFTWARE=1 GDK_BACKEND=x11
