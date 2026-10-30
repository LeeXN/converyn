#!/usr/bin/env python3
"""Private HTTPS/Range fixture for artifact integration tests; never logs credentials."""
import argparse,hmac,http.server,json,os,ssl,threading,time,socketserver
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['cert','key','token-file','events']:p.add_argument('--'+key,type=Path,required=True)
p.add_argument('--file',type=Path);p.add_argument('--directory',type=Path);p.add_argument('--sha256');p.add_argument('--listen',default='127.0.0.1');p.add_argument('--port',type=int,required=True)
a=p.parse_args();token=a.token_file.read_text().strip();lock=threading.Lock();assert bool(a.file)!=bool(a.directory)
class Handler(http.server.BaseHTTPRequestHandler):
 protocol_version='HTTP/1.1'
 def log_message(self,*args):pass
 def do_GET(self):
  if not hmac.compare_digest(self.headers.get('Authorization',''),'Bearer '+token):self.send_error(401);return
  digest=self.path[1:]
  if a.directory:
   if len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):self.send_error(404);return
   file=a.directory/digest
   if not file.is_file() or file.is_symlink():self.send_error(404);return
  else:
   if self.path!='/'+a.sha256:self.send_error(404);return
   file=a.file
  size=file.stat().st_size
  offset=0;range_header=self.headers.get('Range')
  if range_header:
   try:
    assert range_header.startswith('bytes=') and range_header.endswith('-');offset=int(range_header[6:-1]);assert 0<=offset<size
   except (ValueError,AssertionError):self.send_error(416);return
  with lock:
   with a.events.open('a') as f:f.write(json.dumps({'offset':offset,'bytes':size-offset,'at':time.time()})+'\n');f.flush();os.fsync(f.fileno())
  self.send_response(206 if range_header else 200);self.send_header('Content-Length',str(size-offset));self.send_header('ETag','"'+digest+'"')
  if range_header:self.send_header('Content-Range',f'bytes {offset}-{size-1}/{size}')
  self.end_headers()
  try:
   with file.open('rb') as f:
    f.seek(offset)
    while True:
     b=f.read(256*1024)
     if not b:break
     self.wfile.write(b)
  except (BrokenPipeError,ConnectionResetError,ssl.SSLError):pass
class ThreadingServer(socketserver.ThreadingMixIn,http.server.HTTPServer):
 daemon_threads=True
server=ThreadingServer((a.listen,a.port),Handler)
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(a.cert,a.key);server.socket=context.wrap_socket(server.socket,server_side=True)
server.serve_forever()
