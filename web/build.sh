#!/bin/sh
# Builds the wasm client into web/ for local testing and for the GitHub
# Pages deploy workflow (.github/workflows/deploy.yml builds it the same
# way in CI).
#
# The wasm is published under a content-hashed name and the loader reads
# the name from the generated wasm-name.js. Browsers can therefore cache
# the blob aggressively: a rebuild changes the filename (guaranteed fresh
# fetch), an unchanged build keeps it (served from cache, no re-download).
set -e
cd "$(dirname "$0")/.."

cargo build --release -p hibedi-client --target wasm32-unknown-unknown

hash=$(sha256sum target/wasm32-unknown-unknown/release/hibedi-client.wasm | cut -c1-8)
wasm="hibedi-client.$hash.wasm"
cp "target/wasm32-unknown-unknown/release/hibedi-client.wasm" "web/$wasm"

# Drop hashed blobs of earlier builds so the folder holds only the current one.
find web -name 'hibedi-client.*.wasm' ! -name "$wasm" -delete

printf 'window.WASM_PATH = "./%s";\n' "$wasm" > web/wasm-name.js
echo "built web/$wasm"
