#!/usr/bin/env python3
"""Serves a folder to a headset on the same network (the APK, the derived pack) and
appends what the game posts to `?f=<name>` (its log, crash reports, logcat) to files in
`logs/` beside it. For builds made with HORIZON_LOG_URL=http://<this computer>:<port>/log.

Usage: vr_dev_server.py FOLDER [PORT]
"""
import http.server
import os
import sys
import urllib.parse

ROOT = os.path.abspath(sys.argv[1])
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8000
LOGS = os.path.join(ROOT, "logs")
ALLOWED = {"latest.log", "logcat.txt", "crash.txt"}


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=ROOT, **kwargs)

    def do_POST(self):
        url = urllib.parse.urlparse(self.path)
        name = urllib.parse.parse_qs(url.query).get("f", [""])[0]
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(min(length, 16 << 20))
        if url.path != "/log" or name not in ALLOWED:
            self.send_error(404)
            return
        os.makedirs(LOGS, exist_ok=True)
        with open(os.path.join(LOGS, name), "ab") as f:
            f.write(body)
        self.send_response(204)
        self.end_headers()

    def log_message(self, fmt, *args):
        if self.command != "POST":
            super().log_message(fmt, *args)


http.server.ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
