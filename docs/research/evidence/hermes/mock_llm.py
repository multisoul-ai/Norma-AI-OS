import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
LOG = open('/tmp/hermes-probe/mock_requests.log','a')
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_GET(self):
        self.send_response(200); self.send_header('Content-Type','application/json'); self.end_headers()
        self.wfile.write(json.dumps({"object":"list","data":[{"id":"mock-model","object":"model"}]}).encode())
    def do_POST(self):
        n=int(self.headers.get('Content-Length',0)); body=json.loads(self.rfile.read(n) or b'{}')
        msgs=body.get('messages',[])
        LOG.write(json.dumps({"path":self.path,"stream":body.get("stream"),"n":len(msgs),"last":msgs[-1] if msgs else None})[:600]+"\n"); LOG.flush()
        last=msgs[-1] if msgs else {}
        text=str(last.get('content'))
        content=None; tool=None
        if last.get('role')=='tool': content="Tool done: mock final answer."
        elif 'RUNTOOL' in text: tool={"id":"call_1","type":"function","function":{"name":"terminal","arguments":json.dumps({"command":"echo hi-from-tool"})}}
        elif 'DANGER' in text: tool={"id":"call_2","type":"function","function":{"name":"terminal","arguments":json.dumps({"command":"rm -rf /tmp/hermes-probe/danger-dir"})}}
        elif 'ERRFAIL' in text:
            self.send_response(500); self.end_headers(); self.wfile.write(b'{"error":{"message":"mock upstream failure"}}'); return
        elif 'AUTHFAIL' in text:
            self.send_response(401); self.end_headers(); self.wfile.write(b'{"error":{"message":"invalid api key"}}'); return
        elif 'SLOW' in text: time.sleep(25); content="slow answer"
        else: content="MOCK-REPLY to: "+text[:60]
        delta={"role":"assistant"}
        if tool: delta["tool_calls"]=[dict(tool,index=0)]
        else: delta["content"]=content
        fin="tool_calls" if tool else "stop"
        if body.get('stream'):
            self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
            for d,f in ((delta,None),({},fin)):
                self.wfile.write(("data: "+json.dumps({"id":"x","object":"chat.completion.chunk","model":"mock-model","choices":[{"index":0,"delta":d,"finish_reason":f}]})+"\n\n").encode())
            self.wfile.write(("data: "+json.dumps({"id":"x","object":"chat.completion.chunk","model":"mock-model","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}})+"\n\n").encode())
            self.wfile.write(b"data: [DONE]\n\n")
        else:
            msg={"role":"assistant","content":content}
            if tool: msg["tool_calls"]=[tool]
            self.send_response(200); self.send_header('Content-Type','application/json'); self.end_headers()
            self.wfile.write(json.dumps({"id":"x","object":"chat.completion","model":"mock-model","choices":[{"index":0,"message":msg,"finish_reason":fin}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}).encode())
ThreadingHTTPServer(('127.0.0.1',18991),H).serve_forever()
