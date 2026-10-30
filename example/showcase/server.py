#!/usr/bin/python3
"""A small unprivileged HTTP service with verifiable release metadata."""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        body = Path('/opt/converyn-showcase/config.json').read_bytes()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(body)
HTTPServer(('127.0.0.1', 18081), Handler).serve_forever()
