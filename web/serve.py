"""Dev server for web/: a small stand-in for GitHub Pages so local
testing exercises the same caching behavior as the real deployment.

GitHub Pages (Fastly) serves every asset with:

    Cache-Control: max-age=600
    ETag: "<mtime-hex>-<size-hex>"

and honors conditional requests (If-None-Match -> 304 Not Modified, no
body). This server reproduces exactly that: unchanged files — including
the wasm blob — revalidate to a bodyless 304; files within their 10
minute freshness window come straight from the browser cache. A rebuilt
wasm changes its content-hashed filename (see web/build.sh), so it is
always fetched fresh.

Usage: uv run python web/serve.py [port] [max-age-seconds]
Defaults: port 8124, max-age 600 (Pages parity). Pass a max-age of 0
while iterating on HTML/JS to force revalidation on every reload; the
ETag/304 machinery still avoids re-downloading unchanged blobs.
"""

import mimetypes
import os
import socketserver
import sys
from http.server import BaseHTTPRequestHandler
from pathlib import Path

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8124
MAX_AGE = int(sys.argv[2]) if len(sys.argv) > 2 else 600
ROOT = Path(__file__).resolve().parent


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path = Path(os.path.normpath(ROOT / self.path.split("?", 1)[0].lstrip("/")))
        if ROOT not in path.parents and path != ROOT:
            self.send_error(403)
            return
        if path.is_dir():
            path = path / "index.html"
        if not path.is_file():
            self.send_error(404)
            return

        stat = path.stat()
        # Same format GitHub Pages (nginx) uses.
        etag = f'"{stat.st_mtime_ns // 1_000_000_000:x}-{stat.st_size:x}"'
        if self.headers.get("If-None-Match") == etag:
            self.send_response(304)
            self.send_header("ETag", etag)
            self.send_header("Cache-Control", f"max-age={MAX_AGE}")
            self.end_headers()
            return

        self.send_response(200)
        self.send_header("Content-Type", mimetypes.guess_type(path.name)[0] or "application/octet-stream")
        self.send_header("Content-Length", str(stat.st_size))
        self.send_header("ETag", etag)
        self.send_header("Cache-Control", f"max-age={MAX_AGE}")
        self.end_headers()
        with path.open("rb") as file:
            self.wfile.write(file.read())

    def log_message(self, format, *args):
        pass  # keep the dev console quiet


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True


if __name__ == "__main__":
    try:
        server = Server(("", PORT), Handler)
    except OSError as error:
        raise SystemExit(
            f"cannot serve on port {PORT}: {error}\n"
            f"another server is probably still running — "
            f"stop it or pick another port: web/run.sh {PORT + 1}"
        ) from error
    print(f"serving {ROOT} at http://127.0.0.1:{PORT} (Cache-Control: max-age={MAX_AGE})")
    server.serve_forever()
