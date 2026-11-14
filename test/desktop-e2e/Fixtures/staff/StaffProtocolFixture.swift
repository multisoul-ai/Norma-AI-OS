import Foundation

public enum StaffProtocolFixture {
    public static func install() throws -> URL {
        let root = try StaffFixture.install()
        let adapter = root.appendingPathComponent("adapters/node_modules/.bin/claude-agent-acp")
        try Data(script.utf8).write(to: adapter)
        // This scenario needs an addressable Agent, not a Worker execution/hook lifecycle.
        let worker = root.appendingPathComponent("bin/claude")
        let ready = "#!/usr/bin/env python3\nimport json, os, sys\nwith open(os.path.join(os.environ['NORMA_BRAIN_DIR'], 'starts.jsonl'), 'a') as f: f.write(json.dumps({'ready':True})+'\\n')\nprint('PROTOCOL WORKER READY', flush=True)\nsys.stdin.buffer.read()\n"
        try Data(ready.utf8).write(to: worker)
        return root
    }

    private static let script = #"""
    #!/usr/bin/env python3
    import json, os, re, shlex, subprocess, sys, time
    root = os.environ['NORMA_BRAIN_DIR']
    counter = 100
    def send(v): print(json.dumps(v), flush=True)
    def read():
        line = sys.stdin.readline()
        if not line: sys.exit(0)
        return json.loads(line)
    def record(name, value):
        with open(os.path.join(root, name), 'a') as f: f.write(json.dumps(value)+'\n')
    def update(value):
        send(dict(jsonrpc='2.0', method='session/update', params=dict(sessionId='protocol', update=value)))
    def command(text, title):
        global counter
        counter += 1
        update(dict(sessionUpdate='tool_call', toolCallId=str(counter), title=title, status='pending', rawInput=dict(command=text)))
        send(dict(jsonrpc='2.0', id=counter, method='session/request_permission', params=dict(sessionId='protocol',
            toolCall=dict(toolCallId=str(counter), name='Bash', kind='execute', rawInput=dict(command=text)),
            options=[dict(optionId='yes',kind='allow_once'),dict(optionId='no',kind='reject_once')])) )
        answer = read()
        assert answer.get('id') == counter, answer
        allowed = answer.get('result',{}).get('outcome',{}).get('optionId') == 'yes'
        record('commands.jsonl', dict(command=text, allowed=allowed))
        if not allowed:
            update(dict(sessionUpdate='tool_call_update',toolCallId=str(counter),status='failed',rawOutput='The user rejected permission to use this specific tool call.'))
            return None
        result = subprocess.run(shlex.split(text),capture_output=True,text=True,timeout=30)
        assert result.returncode == 0, result.stderr
        update(dict(sessionUpdate='tool_call_update',toolCallId=str(counter),status='completed',rawOutput=result.stdout))
        return json.loads(result.stdout)
    while True:
        message = read(); method, mid = message.get('method'), message.get('id')
        if method == 'initialize': send(dict(jsonrpc='2.0',id=mid,result=dict(protocolVersion=1,agentCapabilities={})))
        elif method == 'session/new':
            assert 'AskUserQuestion' in message['params']['_meta']['claudeCode']['options']['tools']
            send(dict(jsonrpc='2.0',id=mid,result=dict(sessionId='protocol')))
        elif method == 'session/prompt':
            text = ''.join(p.get('text','') for p in message['params']['prompt'])
            record('prompts.jsonl',dict(text=text))
            if text.startswith('Norma command format error'):
                pending = json.loads(text.split('\n')[-1])['unexecuted_commands']
                assert len(pending) == 2, pending
                for index, value in enumerate(pending): command(value, 'Corrected phase' if index == 0 else 'Corrected proposal')
                record('recovered.jsonl',dict(ok=True))
                deadline = time.monotonic()+300
                while not os.path.exists(os.path.join(root,'release-recovery')):
                    if time.monotonic()>deadline: raise RuntimeError('recovery evidence timed out')
                    time.sleep(.05)
                update(dict(sessionUpdate='agent_message_chunk',content=dict(type='text',text='Dark selected. Review proposal ready.')))
            else:
                match = re.search(r'作用域：(\{.*\})\n任务快照：(\{.*\})',text)
                assert match, text
                scope, task = json.loads(match[1]), json.loads(match[2])
                if not any('Question' in row for row in task['messages']):
                    send(dict(jsonrpc='2.0',id=42,method='elicitation/create',params=dict(mode='form',sessionId='protocol',toolCallId='native-question',message='Questions',requestedSchema={
                        'type':'object','properties':{
                            'q0':{'type':'string','title':'Theme','description':'Which interface theme do you prefer?','oneOf':[{'const':'Light','title':'Light','description':'Light interface theme'},{'const':'Dark','title':'Dark','description':'Dark interface theme'}]},
                            'q0_custom':{'type':'string','title':'Theme (other)','description':'Type your own answer'}},'required':[]})))
                    answer = read()
                    assert answer.get('result') == {'action':'decline'}, answer
                    record('question-replies.jsonl',answer)
                else:
                    assert any('Dark' in row.get('User',{}).get('text','') for row in task['messages']), task
                    binary = shlex.split(re.search(r'只能执行 (.*?) brain 命令',text)[1])[0]
                    base = [binary,'brain','--workspace',scope['workspace_id'],'--route',scope['route_id']]
                    roster = command(shlex.join(base+['agents']),'Read agents')
                    agents = json.loads(roster['result']['Brain']['output'])['agents']
                    agent = next(a['name'] for a in agents if a['enabled'])
                    first = base+['task','phase',task['id'],'active']
                    second = base+['task','propose',task['id'],agent,'--title','Review next','--prompt','Review the implementation and tests.','--why','Independent review']
                    assert command(shlex.join(first)+' && '+shlex.join(second),'Combined phase and proposal') is None
                    update(dict(sessionUpdate='agent_message_chunk',content=dict(type='text',text='The user rejected permission.')))
                    send(dict(jsonrpc='2.0',id=mid,result=dict(stopReason='cancelled')))
                    continue
            send(dict(jsonrpc='2.0',id=mid,result=dict(stopReason='end_turn')))
        elif mid is not None: send(dict(jsonrpc='2.0',id=mid,result={}))
    """#
}
