import json
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_POST(self):
        n=int(self.headers.get('Content-Length',0)); b=self.rfile.read(n)
        open('/tmp/hermes-probe/sink.log','a').write(json.dumps({"auth":self.headers.get('Authorization'),"body":json.loads(b)})+"\n")
        self.send_response(204); self.end_headers()
HTTPServer(('127.0.0.1',18992),H).serve_forever()
