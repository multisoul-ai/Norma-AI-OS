import Foundation

enum StaffAdapter {
    static let script = #"""
    #!/usr/bin/env python3
    import json, os, re, shlex, subprocess, sys, time
    root = os.environ['NORMA_BRAIN_DIR']
    def send(value): print(json.dumps(value), flush=True)
    def read():
        line = sys.stdin.readline()
        if not line: sys.exit(0)
        return json.loads(line)
    def record(name, value):
        with open(os.path.join(root, name), 'a') as f: f.write(json.dumps(value) + '\n')
    def update(sid, value):
        send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':value}})
    def hold(stage):
        record('stream-events.jsonl', dict(stage=stage))
        deadline = time.monotonic() + 900
        while not os.path.exists(os.path.join(root, stage)):
            if time.monotonic() > deadline: raise RuntimeError('Fixture release timed out: ' + stage)
            time.sleep(.05)
    counter = 100
    def run(session, argv):
        global counter
        counter += 1
        send({'jsonrpc':'2.0','id':counter,'method':'session/request_permission','params':{
            'sessionId':session,'toolCall':{'toolCallId':str(counter),'name':'Bash','kind':'execute','rawInput':{'command':shlex.join(argv)}},
            'options':[{'optionId':'yes','kind':'allow_once'},{'optionId':'no','kind':'reject_once'}]}})
        while True:
            answer = read()
            if answer.get('id') == counter: break
        allowed = answer.get('result',{}).get('outcome',{}).get('optionId') == 'yes'
        record('commands.jsonl', dict(argv=argv, allowed=allowed))
        if not allowed: return None
        result = subprocess.run(argv, capture_output=True, text=True, timeout=40)
        record('results.jsonl', dict(argv=argv, code=result.returncode, output=result.stdout, error=result.stderr))
        if result.returncode: raise RuntimeError(result.stderr)
        return json.loads(result.stdout)
    while True:
        message = read(); method, mid = message.get('method'), message.get('id')
        if method == 'initialize': send({'jsonrpc':'2.0','id':mid,'result':{'protocolVersion':1,'agentCapabilities':{}}})
        elif method == 'session/new': send({'jsonrpc':'2.0','id':mid,'result':{'sessionId':'staff-shared'}})
        elif method in ('session/set_mode','session/set_model'): send({'jsonrpc':'2.0','id':mid,'result':{}})
        elif method == 'session/prompt':
            text = ''.join(p.get('text','') for p in message['params']['prompt'])
            record('prompts.jsonl', dict(text=text))
            sid = message['params']['sessionId']
            reply = ''
            match = re.search(r'当前作用域：(\{.*\})\n.*?\n(\{.*\})\n只能执行 (.*?) brain 命令', text)
            if match:
                scope, task = json.loads(match[1]), json.loads(match[2])
                binary = shlex.split(match[3])[0]
                base = [binary,'brain','--workspace',scope['workspace_id'],'--route',scope['route_id']]
                tid = task['id']
                users = [m['User']['text'] for m in task['messages'] if 'User' in m]
                last = users[-1] if users else ''
                if 'STREAM-FAIL' in last:
                    send({'jsonrpc':'2.0','id':mid,'error':{'code':-32602,'message':'Fixture permanent failure'}})
                    continue
                elif 'STREAM-ALPHA' in last:
                    hold('release-waiting')
                    update(sid, {'sessionUpdate':'plan','entries':[
                        {'content':'Inspect Alpha project','priority':'high','status':'completed'},
                        {'content':'Prepare acceptance result','priority':'medium','status':'in_progress'}]})
                    update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'ALPHA PARTIAL: project inspected. '}})
                    hold('release-partial')
                    update(sid, {'sessionUpdate':'plan','entries':[
                        {'content':'Inspect Alpha project','priority':'high','status':'completed'},
                        {'content':'Prepare acceptance result','priority':'medium','status':'completed'}]})
                    update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'ALPHA FINAL: acceptance ready.'}})
                    hold('release-final')
                elif 'STREAM-BETA' in last:
                    reply = 'BETA RESULT: separate project conversation.'
                elif not task['proposals']:
                    roster = run(sid, base + ['agents'])
                    # Production CLI wraps Brain output in the response envelope.
                    agents = json.loads(roster['result']['Brain']['output'])['agents']
                    agent = next(a['name'] for a in agents if a['enabled'])
                    run(sid, base + ['task','propose',tid,agent,'--title','实现验收文件','--prompt','STAFF-M1-WORK: 创建 STAFF-RESULT.md 并返回结果。','--why','先完成一个可验收产物'])
                    # Task sessions are forbidden to report: prove the permission gate rejects it.
                    run(sid, base + ['report','--text','TASK MUST NOT SPEAK'])
                    reply = '**STAFF PLAN**：建议先生成验收文件。\n\n- 采用后开始\n- 产物：`STAFF-RESULT.md`'
                elif 'CONFIRM' in last:
                    run(sid, base + ['task','phase',tid,'done'])
                    reply = 'STAFF DONE：验收完成。'
                elif 'MANUAL' in last:
                    run(sid, base + ['task','phase',tid,'merging'])
                    reply = 'STAFF MANUAL：保留你设置的阶段。'
                elif 'REOPEN' in last:
                    reply = 'STAFF REOPEN：继续处理新消息。'
                elif task['workers'] and task['workers'][0].get('outcome') == 'Succeeded':
                    run(sid, base + ['task','phase',tid,'merging'])
                    reply = 'STAFF REVIEW：结果已保存，请验收 STAFF-RESULT.md。'
                if reply:
                    send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':reply}}}})
            send({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'end_turn'}})
        elif mid is not None: send({'jsonrpc':'2.0','id':mid,'result':{}})
    """#
}
