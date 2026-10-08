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
# SETTINGS='{"theme":"cards"}' starts with those settings (the rest
# default). The client's config, home (downloads land in its Downloads)
# and log live in $E2E_DIR (default target/e2e).
set -e
cd "$(dirname "$0")/../.."
dir=${E2E_DIR:-target/e2e}
display=${E2E_DISPLAY:-:77}

case "$1" in
start)
    shift
    cargo build -q -p hibedi-client
    "$0" stop
    rm -rf "$dir"
    mkdir -p "$dir/config/hibedi" "$dir/home/Downloads"
    [ -n "$SETTINGS" ] && printf '%s' "$SETTINGS" > "$dir/config/hibedi/settings.json"
    Xvfb "$display" -screen 0 1400x900x24 >/dev/null 2>&1 &
    # Vulkan on Xvfb presents only through the software WSI.
    DISPLAY=$display WAYLAND_DISPLAY= MESA_VK_WSI_DEBUG=sw \
        XDG_CONFIG_HOME="$dir/config" HOME="$dir/home" \
        target/debug/hibedi-client "$@" > "$dir/client.log" 2>&1 &
    "$0" do wait
    ;;
do)
    shift
    E2E_DISPLAY=$display uv run -q --with python-xlib python tools/e2e/drive.py "$@"
    ;;
stop)
    pkill -x hibedi-client || true
    pkill -f "Xvfb $display" || true
    # Until the display is free again.
    while pgrep -f "Xvfb $display" >/dev/null; do sleep 0.1; done
    ;;
*)
    sed -n '2,24p' "$0"
    exit 2
    ;;
esac
