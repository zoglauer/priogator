#!/usr/bin/env bash
# Build, test and run Priogator on Linux in a podman container (Ubuntu 24.04),
# from a Mac or a Linux machine. The repo is mounted read-only and copied into
# the container; build output and downloaded crates are kept in podman
# volumes, so later runs are fast. Results land in target/podman/.
#
#   scripts/linux-podman.sh setup   # (re)build the container image
#   scripts/linux-podman.sh test    # cargo test, as an ordinary user (default)
#   scripts/linux-podman.sh run     # start the app with demo data on a
#                                   # virtual screen, save a screenshot
#   scripts/linux-podman.sh deb     # build the .deb (needs glibc 2.39; the
#                                   # release builds on 22.04 for older systems)
#   scripts/linux-podman.sh shell   # a shell in the container
#   scripts/linux-podman.sh stop    # stop the podman VM (macOS)
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
image=priogator-linux
out="$repo/target/podman"

command -v podman >/dev/null || {
    echo "podman is missing: brew install podman (macOS) or your package manager." >&2
    exit 1
}

# On macOS, containers run in a Linux VM.
machine() {
    [[ $(uname) == Darwin ]] || return 0
    if ! podman machine inspect >/dev/null 2>&1; then
        podman machine init --cpus "$(sysctl -n hw.ncpu)" --memory 8192 --disk-size 60
    fi
    if [[ $(podman machine inspect --format '{{.State}}') != running ]]; then
        podman machine start
    fi
}

build_image() {
    podman build -t "$image" -f - . <<'CONTAINERFILE'
FROM docker.io/library/ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -qq && apt-get install -y -qq \
        curl git build-essential pkg-config \
        libxkbcommon-dev libgl1-mesa-dev libwayland-dev libx11-dev \
        libxcursor-dev libxrandr-dev libxi-dev libfontconfig-dev \
        xvfb xauth libgl1-mesa-dri libxkbcommon-x11-0 imagemagick \
        fonts-dejavu-core fonts-noto-color-emoji \
    && rm -rf /var/lib/apt/lists/*
# Tests run as an ordinary user: as root, the unreadable-folder tests fail.
RUN useradd -m tester
USER tester
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
    && ~/.cargo/bin/cargo install cargo-deb --locked \
    && mkdir ~/work ~/target
ENV PATH=/home/tester/.cargo/bin:$PATH CARGO_TARGET_DIR=/home/tester/target
WORKDIR /home/tester/work
CONTAINERFILE
}

# Run a command in a fresh container, on a fresh copy of the repo.
in_container() {
    local tty=()
    [[ -t 0 ]] && tty=(-it)
    mkdir -p "$out"
    podman run --rm ${tty[@]+"${tty[@]}"} \
        -v "$repo":/src:ro \
        -v "$out":/out \
        -v priogator-registry:/home/tester/.cargo/registry \
        -v priogator-target:/home/tester/target \
        --user root "$image" bash -c '
            chown tester /home/tester/.cargo/registry /home/tester/target /out
            exec runuser -u tester -- env PATH="$PATH" CARGO_TARGET_DIR="$CARGO_TARGET_DIR" bash -c "
                set -e
                tar -C /src --exclude=./target --exclude=./.git --warning=no-file-changed -cf - . | tar -xf -
                $1"' _ "$1"
}

machine
case "${1:-test}" in
    setup)
        build_image
        exit 0
        ;;
    stop)
        [[ $(uname) == Darwin ]] && podman machine stop
        exit 0
        ;;
esac
podman image exists "$image" || build_image

case "${1:-test}" in
    test)
        in_container 'cargo fetch --locked -q && cargo test'
        ;;
    run)
        in_container '
            Xvfb :9 -screen 0 1280x800x24 >/dev/null 2>&1 &
            sleep 2
            cargo build --release -q
            DISPLAY=:9 $CARGO_TARGET_DIR/release/priogator --demo >/out/run.log 2>&1 &
            sleep 10
            DISPLAY=:9 import -window root /out/screenshot.png
            kill %2 2>/dev/null || true
            echo "Screenshot: target/podman/screenshot.png (log: target/podman/run.log)"'
        ;;
    deb)
        in_container 'cargo deb --locked && cp $CARGO_TARGET_DIR/debian/*.deb /out/ &&
            echo "Package in target/podman/"'
        ;;
    shell)
        in_container 'echo "Repo copy in ~/work; cargo is ready."; bash'
        ;;
    *)
        sed -n 2,14p "$0" | sed 's/^# \{0,1\}//'
        exit 1
        ;;
esac
