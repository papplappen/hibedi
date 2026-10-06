#!/bin/sh
# Builds the wasm client into web/ for local testing and for the GitHub
# Pages deploy workflow (.github/workflows/deploy.yml builds it the same
# way in CI).
#
# wasm-bindgen generates the JS glue for the wasm module; its CLI must
# match the wasm-bindgen crate version in Cargo.lock exactly:
#
#   cargo install wasm-bindgen-cli --version <version> --locked
#
# Glue and wasm are published under one content-hashed name, which the
# page reads from the generated build.js. Browsers can therefore cache
# both aggressively: a rebuild changes the names (guaranteed fresh
# fetch, and never new glue with an old module), an unchanged build
# keeps them.
set -e
cd "$(dirname "$0")/.."

wanted=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
installed=$(wasm-bindgen --version 2>/dev/null | cut -d' ' -f2)
if [ "$installed" != "$wanted" ]; then
    echo "wasm-bindgen CLI ${installed:-not found}; this build needs $wanted:" >&2
    echo "  cargo install wasm-bindgen-cli --version $wanted --locked" >&2
    exit 1
fi

cargo build --profile web -p hibedi-client --target wasm32-unknown-unknown

out=target/web-bindgen
rm -rf "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name hibedi-client \
    target/wasm32-unknown-unknown/web/hibedi-client.wasm

hash=$(cat "$out/hibedi-client.js" "$out/hibedi-client_bg.wasm" | sha256sum | cut -c1-8)
name="hibedi-client.$hash"
# Drop earlier builds so the folder holds only the current one.
find web -name 'hibedi-client.*' -delete
cp "$out/hibedi-client.js" "web/$name.js"
cp "$out/hibedi-client_bg.wasm" "web/$name.wasm"

printf 'window.HIBEDI_BUILD = { glue: "./%s.js", wasm: "./%s.wasm" };\n' "$name" "$name" > web/build.js
echo "built web/$name.{js,wasm} ($(du -h "web/$name.wasm" | cut -f1))"
