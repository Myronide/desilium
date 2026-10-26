# syntax=docker/dockerfile:1
# Multi-stage build for the Desilium Wayland compositor (winit backend).
# Run via scripts/docker-run.sh, which applies the isolation flags.

ARG RUST_IMAGE=rust:1-trixie
ARG RUNTIME_IMAGE=debian:trixie-slim

# ---------------------------------------------------------------------------
# Builder
# ---------------------------------------------------------------------------
FROM ${RUST_IMAGE} AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libwayland-dev \
        libxkbcommon-dev \
        libegl-dev \
        libgles-dev \
        libgbm-dev \
        libdrm-dev \
        libinput-dev \
        libudev-dev \
        libseat-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Cache mounts keep the registry and build artifacts out of the image layers.
# The binary is copied out of the target cache inside the same RUN, because
# cache mounts are not part of the resulting layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    set -eux; \
    if [ -f Cargo.lock ]; then \
        cargo build --release -p desilium-compositor --locked; \
    else \
        cargo build --release -p desilium-compositor; \
    fi; \
    cp target/release/desilium-compositor /usr/local/bin/desilium-compositor; \
    cargo build --release -p desilium-compositor --example layer_client; \
    cp target/release/examples/layer_client /usr/local/bin/layer-client; \
    cargo build --release -p desilium-ui --example layer_demo; \
    cp target/release/examples/layer_demo /usr/local/bin/ui-demo; \
    cargo build --release -p desilium-shell; \
    cp target/release/desilium-shell /usr/local/bin/desilium-shell; \
    cargo build --release -p desilium-settings; \
    cp target/release/desilium-settings /usr/local/bin/desilium-settings

# ---------------------------------------------------------------------------
# Runtime
# ---------------------------------------------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime

# Same Debian release as the builder, so the shared libraries match.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libwayland-client0 \
        libwayland-server0 \
        libwayland-egl1 \
        libxkbcommon0 \
        libegl1 \
        libgles2 \
        libgbm1 \
        libgl1-mesa-dri \
        libinput10 \
        libudev1 \
        libseat1 \
        libdrm2 \
        xkb-data \
        foot \
        hicolor-icon-theme \
        fonts-dejavu-core \
        fonts-inter \
    && rm -rf /var/lib/apt/lists/*

# Unprivileged user matching the uid/gid the run script hardcodes.
RUN groupadd --gid 1000 desilium \
    && useradd --uid 1000 --gid 1000 --no-create-home --home-dir /tmp/home --shell /bin/bash desilium

COPY --from=builder /usr/local/bin/desilium-compositor /usr/local/bin/desilium-compositor
COPY --from=builder /usr/local/bin/layer-client /usr/local/bin/layer-client
COPY --from=builder /usr/local/bin/ui-demo /usr/local/bin/ui-demo
COPY assets/wallpapers /usr/share/desilium/wallpapers
COPY --from=builder /usr/local/bin/desilium-shell /usr/local/bin/desilium-shell
COPY --from=builder /usr/local/bin/desilium-settings /usr/local/bin/desilium-settings
# Fake applications and icons for shell screenshots. NOT on the default XDG_DATA_DIRS:
# pass XDG_DATA_DIRS=/usr/share/desilium-demo:/usr/share and DESILIUM_ICON_THEME=desilium-demo to the shell to see them.
COPY assets/demo-apps /usr/share/desilium-demo

# The root filesystem is mounted read-only at run time; /tmp is the only
# writable location (tmpfs). Mesa and other libraries fall back to $HOME.
ENV LANG=C.UTF-8 \
    HOME=/tmp \
    XDG_CACHE_HOME=/tmp/.cache

USER 1000:1000

ENTRYPOINT ["desilium-compositor"]
CMD ["--command", "foot"]
