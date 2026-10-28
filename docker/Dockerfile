# syntax=docker/dockerfile:1
# Multi-stage build of the Desilium shell and its session, run on KWin (Debian runtime).
# Run via scripts/docker-run.sh, which applies the isolation flags. KWin runs nested in the
# Wayland session of the host.
#
# The KDE package names below (kwin-wayland, kglobalacceld, kf6-kconfig-bin) were written from
# memory and this image has not been built (not in the authoring environment either); check them
# against the Debian release first.
#
# The sibling images docker/Dockerfile.arch, .ubuntu and .fedora are structurally identical (same
# user `dev`, same /home/dev skeleton from docker/home-skel, same ENV, ENTRYPOINT and CMD); only
# the package manager lines differ. Needs BuildKit (COPY --chmod / --chown with build arguments).

ARG RUST_IMAGE=rust:1-trixie
ARG RUNTIME_IMAGE=debian:trixie-slim

# ---------------------------------------------------------------------------
# Builder
# ---------------------------------------------------------------------------
FROM ${RUST_IMAGE} AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libglib2.0-dev \
        libwayland-dev \
        libxkbcommon-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Cache mounts keep the registry and build artifacts out of the image layers.
# The binaries are copied out of the target cache inside the same RUN, because
# cache mounts are not part of the resulting layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    set -eux; \
    cargo build --release --locked \
        -p desilium-session -p desilium-kwin -p desilium-shell \
        -p desilium-notifications -p desilium-settings -p desilium-files; \
    cargo build --release -p desilium-ui --example layer_demo; \
    cp target/release/desilium-session target/release/desilium-kwin \
       target/release/desilium-shell target/release/desilium-notifications \
       target/release/desilium-settings target/release/doria \
       /usr/local/bin/; \
    cp target/release/examples/layer_demo /usr/local/bin/ui-demo

# ---------------------------------------------------------------------------
# Runtime
# ---------------------------------------------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime

# Same Debian release as the builder, so the shared libraries match.
# KWin comes with its Qt stack; dbus-daemon is for the session bus that desilium-session starts
# through dbus-run-session; xwayland lets KWin run X11 applications.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libwayland-client0 \
        libxkbcommon0 \
        libgl1-mesa-dri \
        libegl1 \
        xkb-data \
        kwin-wayland \
        kglobalacceld \
        kf6-kconfig-bin \
        breeze-icon-theme \
        xwayland \
        dbus \
        dbus-daemon \
        kitty \
        foot \
        libglib2.0-0t64 \
        gvfs-backends \
        hicolor-icon-theme \
        fonts-dejavu-core \
        fonts-inter \
    # A cursor theme: without one KWin has no pointer image and the cursor is invisible.
    && for pkg in breeze-cursor-theme adwaita-icon-theme xcursor-themes dmz-cursor-theme; do \
         if apt-cache show "$pkg" >/dev/null 2>&1; then \
           apt-get install -y --no-install-recommends "$pkg" || true; \
         fi; \
       done \
    && { find /usr/share/icons -type d -name cursors | grep -q . \
         || { echo "no cursor theme could be installed" >&2; exit 1; }; } \
    && rm -rf /var/lib/apt/lists/*

# D-Bus (and with it KWin's services and portals) wants a machine id; the slim image ships none.
RUN dbus-uuidgen --ensure=/etc/machine-id

# The development user and its home. scripts/docker-run.sh passes these as build arguments
# (DESILIUM_USER, default dev); the home is the mount point of the persistent Docker volume.
ARG USER_NAME=dev
ARG USER_UID=1000
ARG USER_GID=1000
ARG USER_HOME=/home/${USER_NAME}

RUN groupadd --gid ${USER_GID} ${USER_NAME} \
    && useradd --uid ${USER_UID} --gid ${USER_GID} --create-home --home-dir ${USER_HOME} \
        --shell /bin/bash ${USER_NAME}

COPY --from=builder /usr/local/bin/desilium-session /usr/local/bin/desilium-session
COPY --from=builder /usr/local/bin/desilium-kwin /usr/local/bin/desilium-kwin
COPY --from=builder /usr/local/bin/desilium-shell /usr/local/bin/desilium-shell
COPY --from=builder /usr/local/bin/desilium-notifications /usr/local/bin/desilium-notifications
COPY --from=builder /usr/local/bin/desilium-settings /usr/local/bin/desilium-settings
COPY --from=builder /usr/local/bin/doria /usr/local/bin/doria
COPY --from=builder /usr/local/bin/ui-demo /usr/local/bin/ui-demo
COPY assets/wallpapers /usr/share/desilium/wallpapers
COPY packaging/applications/org.desilium.Shell.desktop /usr/share/applications/org.desilium.Shell.desktop
COPY packaging/applications/org.desilium.Doria.desktop /usr/share/applications/org.desilium.Doria.desktop
COPY packaging/icons/org.desilium.Doria.svg /usr/share/icons/hicolor/scalable/apps/org.desilium.Doria.svg
COPY packaging/icons/desilium-settings.svg /usr/share/icons/hicolor/scalable/apps/desilium-settings.svg
COPY packaging/applications/desilium-settings.desktop /usr/share/applications/desilium-settings.desktop
# Fake applications and icons for shell screenshots. NOT on the default XDG_DATA_DIRS:
# pass XDG_DATA_DIRS=/usr/share/desilium-demo:/usr/share and DESILIUM_ICON_THEME=desilium-demo to the shell to see them.
COPY assets/demo-apps /usr/share/desilium-demo

# The seeded user space: the skeleton from docker/home-skel (XDG user folders, user-dirs.dirs,
# Documents/Welcome.txt), the bundled wallpapers and executable launchers on the desktop (Desilium
# only starts launchers that have the executable bit). scripts/docker-run.sh mounts a named volume
# at the home; Docker fills a new volume from this content once, including ownership.
COPY --chown=${USER_UID}:${USER_GID} docker/home-skel/ ${USER_HOME}/
RUN find ${USER_HOME} -name .gitkeep -type f -delete
COPY --chown=${USER_UID}:${USER_GID} assets/wallpapers/ ${USER_HOME}/Pictures/Wallpapers/
COPY --chown=${USER_UID}:${USER_GID} --chmod=0755 packaging/applications/org.desilium.Doria.desktop ${USER_HOME}/Desktop/org.desilium.Doria.desktop
COPY --chown=${USER_UID}:${USER_GID} --chmod=0755 packaging/applications/desilium-settings.desktop ${USER_HOME}/Desktop/desilium-settings.desktop

# The root filesystem is mounted read-only at run time; the home (a volume) and /tmp (tmpfs) are
# the only writable locations. The XDG directories are the defaults below $HOME.
ENV LANG=C.UTF-8 \
    HOME=${USER_HOME}

USER ${USER_UID}:${USER_GID}
WORKDIR ${USER_HOME}

# The session starts KWin (nested in the host session), the bridge, the shell and the
# notification daemon. --allow-any-client: the shell binary is in /usr/local/bin here, and the
# container is the sandbox, so KWin's per-executable permission check is switched off.
ENTRYPOINT ["desilium-session"]
CMD ["--allow-any-client"]
