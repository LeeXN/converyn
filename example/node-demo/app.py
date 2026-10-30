#!/usr/bin/env python3
"""Small local HTTP service serving an atomically selected release directory."""
import http.server
import json
from pathlib import Path

ROOT = Path('/opt/converyn-node-demo/data')
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        try:
            if self.path == '/ready':
                data = b'{"ready":true}'
            elif self.path == '/health':
                data = (ROOT / 'current' / 'release.json').read_bytes()
            elif self.path == '/':
                data = (ROOT / 'current' / 'index.html').read_bytes()
            else:
                self.send_error(404); return
            self.send_response(200)
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        except (FileNotFoundError, PermissionError):
            self.send_error(503, 'Release unavailable')

http.server.HTTPServer(('127.0.0.1', 18080), Handler).serve_forever()
