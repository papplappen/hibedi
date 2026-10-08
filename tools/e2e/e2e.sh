#!/bin/sh
# End-to-end checks of the native client, on a virtual display of its
# own (Xvfb), isolated from the desktop and from the player's settings
# and files. Needs Xvfb, ImageMagick (import) and uv.
#
#   tools/e2e/e2e.sh start [client args]   build, then start the client
#   tools/e2e/e2e.sh do <commands>         drive it (see drive.py)
#   tools/e2e/e2e.sh stop
#
# Client args reach positions directly: --seed <n> deals the same boards
# every time, --game <file> continues a game in notation, e.g. one the
# bots play:
#
#   cargo run -q -p hibedi-bots --example selfplay -- --seed 1 --turns 3 > /tmp/g.txt
#   tools/e2e/e2e.sh start --game /tmp/g.txt
#   tools/e2e/e2e.sh do key:1 sleep:1 shot:/tmp/after.png
#
# E2E_SCALE=1.25 runs the client at that scale factor (as on a scaled
# display). Settings are client args too (--theme cards, --animations none, ...;
# see hibedi-client --help); the rest start at their defaults. The
# client's config, home (downloads land in its Downloads) and log live
# in $E2E_DIR (default target/e2e).
set -e
cd "$(dirname "$0")/../.."
# Absolute: relative XDG paths do not count (and fall back to $HOME).
dir=$(realpath -m "${E2E_DIR:-target/e2e}")
display=${E2E_DISPLAY:-:77}

case "$1" in
start)
    shift
    cargo build -q -p hibedi-client
    "$0" stop
    rm -rf "$dir"
    mkdir -p "$dir/config" "$dir/home/Downloads"
    Xvfb "$display" -screen 0 1920x1200x24 >/dev/null 2>&1 &
    echo $! > "$dir/xvfb.pid"
    "$0" do  # until the display takes clients
    # Vulkan on Xvfb presents only through the software WSI.
    DISPLAY=$display WAYLAND_DISPLAY= MESA_VK_WSI_DEBUG=sw \
        WINIT_X11_SCALE_FACTOR=${E2E_SCALE:-1} \
        XDG_CONFIG_HOME="$dir/config" HOME="$dir/home" \
        target/debug/hibedi-client "$@" > "$dir/client.log" 2>&1 &
    echo $! > "$dir/client.pid"
    "$0" do wait
    ;;
do)
    shift
    E2E_DISPLAY=$display uv run -q --with python-xlib python tools/e2e/drive.py "$@"
    ;;
stop)
    # Only what start started: never other instances of the game.
    for name in client xvfb; do
        file="$dir/$name.pid"
        [ -f "$file" ] || continue
        pid=$(cat "$file")
        kill "$pid" 2>/dev/null || true
        # Until it is gone (the display is free again).
        while kill -0 "$pid" 2>/dev/null; do sleep 0.1; done
        rm -f "$file"
    done
    ;;
*)
    sed -n '2,25p' "$0"
    exit 2
    ;;
esac
