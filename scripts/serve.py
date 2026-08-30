#!/usr/bin/env python3
"""Serve repository web assets with cross-origin isolation headers."""

from argparse import ArgumentParser
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class IsolatedHandler(SimpleHTTPRequestHandler):
    isolation = True

    def end_headers(self) -> None:
        if self.isolation:
            self.send_header("Cross-Origin-Opener-Policy", "same-origin")
            self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
            self.send_header("Cross-Origin-Resource-Policy", "same-origin")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def main() -> None:
    parser = ArgumentParser()
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--directory", type=Path, default=Path("web"))
    parser.add_argument(
        "--no-isolation",
        action="store_true",
        help="omit COOP/COEP (needed by Perfetto's cross-origin embedding API)",
    )
    args = parser.parse_args()
    IsolatedHandler.isolation = not args.no_isolation
    handler = lambda *a, **kw: IsolatedHandler(*a, directory=str(args.directory), **kw)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), handler)
    print(f"serving http://127.0.0.1:{args.port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
