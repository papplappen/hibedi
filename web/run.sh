#!/bin/sh
# Build the wasm client and serve web/ locally, optionally opening a
# browser.
#
#   web/run.sh              build + serve on port 8124
#   web/run.sh --open       also open the page in this machine's browser
#   web/run.sh --open 9000  ... on a different port
#
# Two-machine test: on the other machine, open the "other machine" URL
# printed below. Create the room from THAT URL on this machine too (not
# localhost) — the invite link is built from the page's own address, so
# a localhost link would send the other player to their own machine.
# If the other machine cannot load the page, check this machine's
# firewall allows inbound connections on the port.
set -e
cd "$(dirname "$0")/.."

open=false
port=8124
for arg in "$@"; do
    case "$arg" in
        --open) open=true ;;
        *) port=$arg ;;
    esac
done

web/build.sh

py="python3"
command -v uv >/dev/null && py="uv run python"
lan_ip=$($py -c "import socket
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.connect(('8.8.8.8', 80))
print(s.getsockname()[0])" 2>/dev/null || echo "<this-machine-ip>")
echo
echo "  this machine:  http://localhost:$port"
echo "  other machine: http://$lan_ip:$port"
echo

if $open; then
    xdg-open "http://localhost:$port" >/dev/null 2>&1 &
fi

if command -v uv >/dev/null; then
    exec uv run python web/serve.py "$port"
else
    exec python3 web/serve.py "$port"
fi
