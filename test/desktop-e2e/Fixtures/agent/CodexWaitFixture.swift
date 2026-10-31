import E2EKit
import Foundation

/// 真实 Codex + 本机 Responses 服务；只控制响应时机，不伪造 CLI 或原生 turn 事件。
@MainActor
public final class CodexWaitFixture {
    public let root: URL
    public let home: URL
    private let provider = Process()
    private var log: FileHandle?

    public init(root: URL, hookBinary: String) async throws {
        self.root = root
        home = root.appendingPathComponent("home")
        let config = home.appendingPathComponent(".codex")
        try FileManager.default.createDirectory(at: config, withIntermediateDirectories: true)
        try write(Self.providerScript, "provider.py")
        try write(Self.hookScript, "hook.py")
        try write(Self.brainScript, "adapters/node_modules/.bin/claude-agent-acp", executable: true)
        try write(#"{"enabled":true,"backend":"claude","strategy":"speed","speak":false,"autonomy":{"assign_followups":false}}"#, "config.json")
        try write("import pathlib,time\nr=pathlib.Path(__file__).parent\nprint('BACKGROUND_TERMINAL_START',flush=True)\nwhile not (r/'finish-terminal').exists():time.sleep(.1)\nprint('BACKGROUND_TERMINAL_DONE',flush=True)\n", "terminal.py")
        try write("export CODEX_HOME='\(config.path)'\n", ".zshrc")
        let logURL = root.appendingPathComponent("provider.log")
        FileManager.default.createFile(atPath: logURL.path, contents: nil)
        log = try FileHandle(forWritingTo: logURL)
        provider.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        provider.arguments = [root.appendingPathComponent("provider.py").path, root.path]
        provider.standardOutput = log
        provider.standardError = log
        try provider.run()
        try await waitFile("port")
        let port = try String(contentsOf: root.appendingPathComponent("port"), encoding: .utf8)
        let toml = """
        model="gpt-5.5"
        model_provider="wait-test"
        approval_policy="never"
        sandbox_mode="danger-full-access"
        check_for_update_on_startup=false
        suppress_unstable_features_warning=true
        [features]
        codex_hooks=true
        [model_providers.wait-test]
        name="Wait Test"
        base_url="http://127.0.0.1:\(port)/v1"
        wire_api="responses"
        requires_openai_auth=false
        supports_websockets=false
        """
        try Data((toml + "\n").utf8).write(to: config.appendingPathComponent("config.toml"))
        let command = "/usr/bin/python3 '\(root.appendingPathComponent("hook.py").path)' '\(hookBinary)' forward --source codex"
        let hooks = Dictionary(uniqueKeysWithValues:
            ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Stop", "SubagentStart", "SubagentStop"].map {
                ($0, [["hooks": [["type": "command", "command": command]]]])
            })
        try JSONSerialization.data(withJSONObject: ["hooks": hooks]).write(to: config.appendingPathComponent("hooks.json"))
        for key in ProcessInfo.processInfo.environment.keys where key.hasPrefix("MULTISOUL_") || key.hasPrefix("CODEX_") || key.hasPrefix("OPENAI_") { unsetenv(key) }
        setenv("CODEX_HOME", config.path, 1)
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("NORMA_BRAIN_QUIET_SECS", "1", 1)
        setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
    }

    public func trust(project: URL) throws {
        let url = home.appendingPathComponent(".codex/config.toml")
        let old = try String(contentsOf: url, encoding: .utf8)
        try Data((old + "\n[projects.\"\(project.path)\"]\ntrust_level=\"trusted\"\n").utf8).write(to: url)
    }

    public func release(_ file: String) throws { try Data().write(to: root.appendingPathComponent(file)) }

    public func waitFile(_ file: String, timeout: Double = 15) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while !FileManager.default.fileExists(atPath: root.appendingPathComponent(file).path) {
            guard Date() < deadline else { throw E2EError.product("未收到夹具证据 \(file)") }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
    }

    public func waitPhase(_ phase: String, timeout: Double = 30) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let data = try? Data(contentsOf: root.appendingPathComponent("phase.json")),
               let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                if value["phase"] as? String == phase { return }
                if value["phase"] as? String == "error" { throw E2EError.product("Codex API 夹具错误：\(value)") }
            }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("真实 Codex 未进入 \(phase)")
    }

    public func finished() -> [String] {
        ((try? String(contentsOf: root.appendingPathComponent("brain-finished.jsonl"), encoding: .utf8)) ?? "")
            .split(separator: "\n").map(String.init)
    }

    public func stop() {
        try? release("stop-watchers")
        if provider.isRunning { provider.terminate(); provider.waitUntilExit() }
        try? log?.close()
    }

    private func write(_ text: String, _ file: String, executable: Bool = false) throws {
        let url = root.appendingPathComponent(file)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data((text + "\n").utf8).write(to: url)
        if executable { try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path) }
    }

    private static let hookScript = #"""
    import json, os, pathlib, subprocess, sys, time, urllib.request
    root=pathlib.Path(__file__).parent
    if len(sys.argv)>1 and sys.argv[1]=='watch':
        raw=json.load(sys.stdin)
        end=time.time()+120
        while not (root/'inject-stop').exists():
            if time.time()>end or (root/'stop-watchers').exists():sys.exit(0)
            time.sleep(.05)
        raw.update(hook_event_name='Stop',last_assistant_message='PREMATURE_STOP_SENTINEL',_norma_control_run_id=os.environ.get('NORMA_CONTROL_RUN_ID'))
        body=dict(node_id=int(os.environ['MULTISOUL_NODE_ID']),profile='Codex',provider_event='Stop',session_id=raw.get('session_id'),turn_id=raw.get('turn_id'),control_run_id=os.environ.get('NORMA_CONTROL_RUN_ID'),tool_name=None,message=None,raw_json=raw)
        req=urllib.request.Request(os.environ['MULTISOUL_HOOK_ENDPOINT'],json.dumps(body).encode(),{'Authorization':'Bearer '+os.environ['MULTISOUL_HOOK_TOKEN'],'Content-Type':'application/json'})
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=5) as response:
            (root/'injected.json').write_text(json.dumps(dict(status=response.status,test_injection=True,session_id=raw.get('session_id'),turn_id=raw.get('turn_id'))))
        sys.exit(0)
    data=sys.stdin.buffer.read();raw=json.loads(data)
    with (root/'hooks.jsonl').open('a') as f:f.write(json.dumps({k:raw.get(k) for k in ('hook_event_name','session_id','turn_id','tool_name')})+'\n')
    if raw.get('hook_event_name')=='UserPromptSubmit':
        p=subprocess.Popen(['/usr/bin/python3',__file__,'watch'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
        p.stdin.write(data);p.stdin.close()
    sys.exit(subprocess.run(sys.argv[1:],input=data).returncode)
    """#

    private static let brainScript = #"""
    #!/usr/bin/env python3
    import json, os, pathlib, re, sys, time
    root=pathlib.Path(os.environ['NORMA_BRAIN_DIR'])
    def send(o):print(json.dumps(o),flush=True)
    for line in sys.stdin:
        m=json.loads(line);method=m.get('method');mid=m.get('id')
        if method=='initialize':result={'protocolVersion':1,'agentCapabilities':{}}
        elif method=='session/new':result={'sessionId':'codex-wait-observer'}
        elif method=='session/set_mode':result={}
        elif method=='session/prompt':
            text=''.join(p.get('text','') for p in m['params']['prompt'])
            with (root/'brain-prompts.jsonl').open('a') as f:f.write(json.dumps(dict(at=time.time(),text=text),ensure_ascii=False)+'\n')
            if ('【事件】' in text or '【静默记录】' in text) and re.search(r'^- .+ (?:已完成|失败|被中断)：',text,re.M):
                with (root/'brain-finished.jsonl').open('a') as f:f.write(json.dumps(dict(at=time.time(),text=text),ensure_ascii=False)+'\n')
                send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'codex-wait-observer','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'CODEX_BRAIN_COMPLETION_RECEIVED'}}}})
            result={'stopReason':'end_turn'}
        else:result={}
        if mid is not None:send({'jsonrpc':'2.0','id':mid,'result':result})
    """#

    private static let providerScript = #"""
    #!/usr/bin/env python3
    import http.server, json, pathlib, re, sys, threading, time
    ROOT=pathlib.Path(sys.argv[1]); ROOT.mkdir(parents=True,exist_ok=True)
    lock=threading.Lock(); requests=0

    def phase(name, **fields):
        with (ROOT/'provider-events.jsonl').open('a') as f:f.write(json.dumps(dict(phase=name,at=time.time(),**fields))+'\n')
        (ROOT/'phase.json').write_text(json.dumps(dict(phase=name,**fields)))
    def gate(name):
        end=time.time()+180
        while not (ROOT/name).exists():
            if time.time()>end:raise TimeoutError(name)
            time.sleep(.05)

    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'
        def log_message(self,*args):pass
        def do_GET(self):
            body=json.dumps({'data':[{'id':'gpt-5.5','object':'model'}]}).encode()
            self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        def do_POST(self):
            global requests
            try:
                body=json.loads(self.rfile.read(int(self.headers.get('Content-Length',0))))
                with lock: requests+=1; n=requests
                phase('request',number=n,tools=[{'type':t.get('type'),'name':t.get('name')} for t in body.get('tools',[])])
                self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Connection','close');self.end_headers()
                self.event('response.created',response={'id':'resp_'+str(n),'object':'response','status':'in_progress','output':[]})
                if n==1:
                    phase('api_wait');gate('release-api')
                    item={'id':'rs_1','type':'reasoning','summary':[]}
                    self.event('response.output_item.added',output_index=0,item=item)
                    self.event('response.reasoning_summary_part.added',item_id='rs_1',output_index=0,summary_index=0,part={'type':'summary_text','text':''})
                    self.event('response.reasoning_summary_text.delta',item_id='rs_1',output_index=0,summary_index=0,delta='Checking the Codex wait-state regression')
                    phase('thinking');gate('release-thinking')
                    self.event('response.output_item.done',output_index=0,item={'id':'rs_1','type':'reasoning','summary':[{'type':'summary_text','text':'Checking the Codex wait-state regression'}]})
                    self.tool(n,'exec_command',{'cmd':'/usr/bin/python3 '+str(ROOT/'terminal.py'),'yield_time_ms':1000,'max_output_tokens':1000},index=1)
                elif n==2:
                    text=json.dumps(body)
                    ids=re.findall(r'(?:session ID|session id)\s+(\d+)',text)
                    if not ids:raise ValueError('no background session id; '+text[-1200:])
                    phase('terminal_wait',session_id=ids[-1])
                    item={'id':'msg_wait','type':'message','role':'assistant','phase':'commentary','status':'completed','content':[{'type':'output_text','text':'Waiting for background terminal','annotations':[]}]}
                    self.event('response.output_item.added',output_index=0,item=dict(item,content=[]))
                    self.event('response.output_text.delta',item_id='msg_wait',output_index=0,content_index=0,delta='Waiting for background terminal')
                    self.event('response.output_item.done',output_index=0,item=item)
                    self.tool(n,'write_stdin',{'session_id':int(ids[-1]),'chars':'','yield_time_ms':120000,'max_output_tokens':1000},index=1)
                else:
                    phase('final_api_wait');gate('release-final')
                    text='CODEX_WAIT_COMPLETE：后台终端已结束，需要你确认验收结果。'
                    item={'id':'msg_final','type':'message','role':'assistant','phase':'final_answer','status':'completed','content':[{'type':'output_text','text':text,'annotations':[]}]}
                    self.event('response.output_item.added',output_index=0,item=dict(item,content=[]))
                    self.event('response.output_text.delta',item_id='msg_final',output_index=0,content_index=0,delta=text)
                    self.event('response.output_item.done',output_index=0,item=item)
                    self.complete(n,[item]);phase('completed')
            except Exception as e:
                phase('error',error=str(e));self.close_connection=True
        def event(self,kind,**data):
            self.wfile.write(('data: '+json.dumps(dict(type=kind,**data))+'\n\n').encode());self.wfile.flush()
        def tool(self,n,name,args,index=0):
            args=json.dumps(args);item={'id':'fc_'+str(n),'type':'function_call','call_id':'call_'+str(n),'name':name,'arguments':''}
            self.event('response.output_item.added',output_index=index,item=item)
            self.event('response.function_call_arguments.delta',item_id=item['id'],output_index=index,delta=args)
            item['arguments']=args
            self.event('response.output_item.done',output_index=index,item=item)
            self.complete(n,[item])
        def complete(self,n,items):
            self.event('response.completed',response={'id':'resp_'+str(n),'object':'response','status':'completed','output':items,'usage':{'input_tokens':100,'output_tokens':20,'total_tokens':120}})
            self.close_connection=True

    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
    (ROOT/'port').write_text(str(server.server_port));phase('ready',port=server.server_port)
    server.serve_forever()
    """#
}
