# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Self-hosted GitHub Actions runner able to run .github/workflows/build.yml.
# Build and launch it with scripts/run_ci_runner.sh. The build context is the .github directory.
#
# Everything third-party is pinned to a release at least two weeks old. Bump them deliberately.
# The Rust toolchain is not pinned here: rust-toolchain.toml selects it and rustup installs it on
# the first cargo invocation (cached in a volume by scripts/run_ci_runner.sh).

# ubuntu:noble-20260917 (pushed 2026-10-04)
FROM docker.io/library/ubuntu:noble-20260917

# actions/runner v2.337.0, released 2026-08-26. The checksum is the one published in the release notes.
ARG RUNNER_VERSION=2.337.0
ARG RUNNER_SHA256=70920811a4f8ad4328818682bca5c6469c1c942fab52448868071d0063816613
# rustup 1.29.1, tagged 2026-08-13. The checksum is published next to the binary on static.rust-lang.org.
ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
# Qt 6.12.0 packages from download.qt.io, released 2026-09-28
ARG QT_VERSION=6.12.0
# aqtinstall 3.3.0 (2025-06-02), reuse 6.2.0 (2025-10-27)
ARG AQTINSTALL_VERSION=3.3.0
ARG REUSE_VERSION=6.2.0

ENV DEBIAN_FRONTEND=noninteractive

# Build tools, what vcpkg's bootstrap needs (curl, zip, unzip, tar, pkg-config, git), and the
# libraries Qt needs to build and to run with QT_QPA_PLATFORM=offscreen or xcb. lld because cxx-qt
# otherwise falls back to the deprecated gold linker.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential g++-14 gcc-14 lld cmake ninja-build pkg-config git curl ca-certificates \
        zip unzip tar xz-utils perl python3 python3-venv python3-pip \
        libgl-dev libegl-dev libopengl-dev libgles-dev libglx-dev libgl1 libegl1 libgles2 libgl1-mesa-dri \
        libxkbcommon-dev libxkbcommon-x11-0 libfontconfig1-dev libfreetype-dev libdbus-1-3 libglib2.0-0t64 \
        libx11-dev libx11-xcb-dev libxext6 libxrender1 libxi6 \
        libxcb1-dev libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 libxcb-randr0 \
        libxcb-render-util0 libxcb-shape0 libxcb-shm0 libxcb-sync1 libxcb-xfixes0 libxcb-xinerama0 \
        libxcb-xkb1 libxcb-util1 \
    && rm -rf /var/lib/apt/lists/*

# The image's default user would take uid 1000; the runner gets it instead.
RUN userdel -r ubuntu \
    && useradd -m -u 1000 -s /bin/bash runner

# Python tools in a venv (Ubuntu's Python is externally managed).
RUN python3 -m venv /opt/venv \
    && /opt/venv/bin/pip install --no-cache-dir aqtinstall==${AQTINSTALL_VERSION} reuse==${REUSE_VERSION}

# Qt is on PATH, which is how the build finds it (find_package and cxx-qt's qmake lookup). Its
# libraries are registered with the dynamic loader, because test binaries built by cargo (rc_qt's)
# have no rpath to them.
RUN /opt/venv/bin/aqt install-qt linux desktop ${QT_VERSION} linux_gcc_64 --outputdir /opt/Qt \
    && test -x /opt/Qt/${QT_VERSION}/gcc_64/bin/qmake \
    && echo /opt/Qt/${QT_VERSION}/gcc_64/lib > /etc/ld.so.conf.d/qt.conf \
    && ldconfig

ENV PATH=/opt/Qt/${QT_VERSION}/gcc_64/bin:/opt/venv/bin:$PATH
ENV CC=gcc-14 CXX=g++-14

# The runner, plus its own OS dependencies (libicu and friends, needs root).
RUN mkdir -p /opt/actions-runner && cd /opt/actions-runner \
    && curl -fsSL -o runner.tar.gz \
        https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz \
    && echo "${RUNNER_SHA256}  runner.tar.gz" | sha256sum -c - \
    && tar xzf runner.tar.gz && rm runner.tar.gz \
    && apt-get update && ./bin/installdependencies.sh \
    && rm -rf /var/lib/apt/lists/* \
    && chown -R runner:runner /opt/actions-runner

USER runner
WORKDIR /home/runner

# Cache locations. scripts/run_ci_runner.sh mounts volumes on the ones below, so they are created
# here, owned by the runner, which gives empty volumes the right owner.
ENV CARGO_HOME=/home/runner/.cargo \
    RUSTUP_HOME=/home/runner/.rustup \
    VCPKG_DOWNLOADS=/home/runner/.cache/vcpkg/downloads \
    VCPKG_DEFAULT_BINARY_CACHE=/home/runner/.cache/vcpkg/archives
ENV PATH=/home/runner/.cargo/bin:$PATH
RUN mkdir -p .cargo/registry .cargo/git .rustup "$VCPKG_DOWNLOADS" "$VCPKG_DEFAULT_BINARY_CACHE" /opt/actions-runner/_work

# rustup only. No toolchain: rust-toolchain.toml decides, rustup installs it on first use.
RUN curl -fsSL -o /tmp/rustup-init \
        https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init \
    && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none \
    && rm /tmp/rustup-init

COPY --chown=runner:runner runner-entrypoint.sh /home/runner/runner-entrypoint.sh
RUN chmod +x /home/runner/runner-entrypoint.sh

WORKDIR /opt/actions-runner
ENTRYPOINT ["/home/runner/runner-entrypoint.sh"]
