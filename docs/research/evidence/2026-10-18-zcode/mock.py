import json, time, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
LOG=open('/tmp/zc-hooks/mock-requests.log','a')
def sse(w, obj):
    w.write(("data: "+json.dumps(obj)+"\n\n").encode()); w.flush()
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_GET(self):
        self.send_response(200); self.send_header('content-type','application/json'); self.end_headers()
        self.wfile.write(json.dumps({"object":"list","data":[{"id":"mock-model","object":"model"}]}).encode())
    def do_POST(self):
        n=int(self.headers.get('content-length',0)); body=json.loads(self.rfile.read(n) or b'{}')
        msgs=body.get('messages',[])
        LOG.write(json.dumps({"path":self.path,"nmsgs":len(msgs),"last_role":msgs[-1]['role'] if msgs else None,"stream":body.get('stream')})+"\n"); LOG.flush()
        last=msgs[-1] if msgs else {}
        users=[m for m in msgs if m['role']=='user']
        utext=json.dumps(users[-1]['content']) if users else ''
        if 'FAILME' in utext:
            self.send_response(500); self.send_header('content-type','application/json'); self.end_headers(); self.wfile.write(b'{"error":{"message":"boom"}}'); return
        self.send_response(200); self.send_header('content-type','text/event-stream'); self.end_headers()
        base={"id":"c1","object":"chat.completion.chunk","created":1,"model":"mock-model"}
        def ch(delta,fin=None): return dict(base,choices=[{"index":0,"delta":delta,"finish_reason":fin}])
        if 'SLOW' in utext and last['role']!='tool':
            sse(self.wfile,ch({"role":"assistant","content":"working "}))
            time.sleep(20)
        if last['role']=='tool':
            sse(self.wfile,ch({"role":"assistant","content":"tool finished OK"}))
            sse(self.wfile,ch({},"stop"))
        elif 'RUNTOOL' in utext or 'RUNWRITE' in utext or 'RUNREAD' in utext:
            sse(self.wfile,ch({"role":"assistant","tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":("Read" if "RUNREAD" in utext else "Bash"),"arguments":""}}]}))
            if 'RUNWRITE' in utext: tn,args='Bash',json.dumps({"command":"touch /tmp/zc-work/approve.txt","description":"touch"})
            elif 'RUNREAD' in utext: tn,args='Read',json.dumps({"file_path":"/tmp/zc-work/nonexistent-file.txt"})
            else: tn,args='Bash',json.dumps({"command":"echo hi-from-tool","description":"echo"})
            sse(self.wfile,ch({"tool_calls":[{"index":0,"function":{"arguments":args}}]}))
            sse(self.wfile,ch({},"tool_calls"))
        else:
            sse(self.wfile,ch({"role":"assistant","content":"mock reply: hello"}))
            sse(self.wfile,ch({},"stop"))
        sse(self.wfile,dict(base,choices=[],usage={"prompt_tokens":5,"completion_tokens":3,"total_tokens":8}))
        self.wfile.write(b"data: [DONE]\n\n"); self.wfile.flush()
ThreadingHTTPServer(('127.0.0.1',18765),H).serve_forever()
