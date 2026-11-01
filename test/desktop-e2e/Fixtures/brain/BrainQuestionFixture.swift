import Foundation

/// UI 验收夹具：执行生产注入的 Hook 命令，消费其 updatedInput 后再发原 tool_use_id 回执。
/// 使用脚本化 Agent/幕僚，不将此夹具计作真实 Claude CLI 或真实模型验收。
public enum BrainQuestionFixture {
    public static func install() throws -> (shell: URL, brain: URL) {
        let brain = try BrainFixture.installScriptedBrainDirectory(mode: .propose)
        let shell = URL(fileURLWithPath: "/private/tmp").appendingPathComponent("norma-question-ui-\(UUID().uuidString)")
        let bin = shell.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\n".utf8).write(to: shell.appendingPathComponent(".zshrc"))
        for (path, script) in [
            (bin.appendingPathComponent("claude"), agent),
            (brain.appendingPathComponent("adapters/node_modules/.bin/claude-agent-acp"), adapter)
        ] {
            try Data(script.utf8).write(to: path)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        }
        return (shell, brain)
    }

    private static let agent = #"""
    #!/usr/bin/env python3
    import json, os, shlex, subprocess, sys, threading, time
    root = os.environ['NORMA_BRAIN_DIR']
    settings = json.loads(sys.argv[sys.argv.index('--settings') + 1])
    sid = 'question-ui-' + os.environ['NORMA_CONTROL_RUN_ID']
    def hook(name, raw):
        raw = dict(raw, hook_event_name=name, session_id=sid)
        output = None
        for group in settings['hooks'].get(name, []):
            if group.get('matcher') not in (None, '', '*', raw.get('tool_name')): continue
            for item in group['hooks']:
                result = subprocess.run(shlex.split(item['command']), input=json.dumps(raw),
                    capture_output=True, text=True, timeout=130)
                with open(root + '/hook.log', 'a') as f: f.write(name + ': ' + result.stderr + '\n')
                if result.stdout.strip(): output = json.loads(result.stdout)
        return output
    def record_input():
        for line in sys.stdin:
            with open(root + '/ordinary-input.jsonl', 'a') as f: f.write(json.dumps(line) + '\n')
    threading.Thread(target=record_input, daemon=True).start()
    hook('SessionStart', {'source':'startup'})
    print('Question UI fixture ready', flush=True)
    for cycle in [1, 2, 3, 4]:
        while not os.path.exists(root + '/question-' + str(cycle)): time.sleep(.1)
        prompt = 'Choose color ' + str(cycle)
        questions = [{'question':prompt,'header':'Color','options':[
            {'label':'Blue','description':'Blue color'}, {'label':'Green','description':'Green color'}], 'multiSelect':False}]
        call = {'tool_name':'AskUserQuestion','tool_use_id':'question-ui-tool-' + str(cycle),'tool_input':{'questions':questions}}
        output = hook('PreToolUse', call)
        if not output:
            print('Native answer required', flush=True)
            with open(root + '/question-failed', 'w') as f: f.write(str(cycle))
            break
        updated = output['hookSpecificOutput']['updatedInput']
        answer = updated['answers'][prompt]
        with open(root + '/consumed.jsonl', 'a') as f:
            f.write(json.dumps({'cycle':cycle,'tool_use_id':call['tool_use_id'],'answers':updated['answers']}) + '\n')
        print('ANSWER_RECEIVED ' + answer, flush=True)
        hook('PostToolUse', dict(call, tool_input=updated, tool_response=updated))
    while True: time.sleep(1)
    """#

    private static let adapter = #"""
    #!/usr/bin/env python3
    import json, os, re, shlex, subprocess, sys
    root = os.environ['NORMA_BRAIN_DIR']
    exe = None
    def send(value): print(json.dumps(value), flush=True)
    def read():
        line = sys.stdin.readline()
        if not line: sys.exit(0)
        return json.loads(line)
    request_id = 100
    def execute(argv, session):
        global request_id
        request_id += 1
        send({'jsonrpc':'2.0','id':request_id,'method':'session/request_permission','params':{
            'sessionId':session,'toolCall':{'toolCallId':str(request_id),'name':'Bash','kind':'execute','rawInput':{'command':shlex.join(argv)}},
            'options':[{'optionId':'once','kind':'allow_once'},{'optionId':'reject','kind':'reject_once'}]}})
        while True:
            response = read()
            if response.get('id') == request_id:
                if response.get('result',{}).get('outcome',{}).get('optionId') != 'once': raise RuntimeError('permission rejected')
                break
        result = subprocess.run(argv, capture_output=True, text=True, timeout=20)
        with open(root + '/question-commands.jsonl','a') as f:
            f.write(json.dumps({'argv':argv[1:],'returncode':result.returncode,'stdout':result.stdout,'stderr':result.stderr}) + '\n')
        if result.returncode: raise RuntimeError(result.stderr)
        return json.loads(result.stdout)
    while True:
        message = read(); method = message.get('method'); mid = message.get('id')
        if method == 'initialize': send({'jsonrpc':'2.0','id':mid,'result':{'protocolVersion':1,'agentCapabilities':{}}})
        elif method == 'session/new': send({'jsonrpc':'2.0','id':mid,'result':{'sessionId':'question-ui-brain'}})
        elif method == 'session/set_mode': send({'jsonrpc':'2.0','id':mid,'result':{}})
        elif method == 'session/prompt':
            text = ''.join(p.get('text','') for p in message['params']['prompt'])
            found = re.search(r'`([^`]+) brain agents`', text)
            if found: exe = shlex.split(found.group(1))[0]
            with open(root + '/question-prompt.log','a') as f: f.write(text + '\n')
            try:
                if exe and '【原问题待答】' in text:
                    response = execute([exe,'brain','questions'],'question-ui-brain')
                    snapshots = json.loads(response['result']['Brain']['output'])['questions']
                    for snapshot in snapshots:
                        if snapshot['state'] != 'pending': continue
                        question = snapshot['question']
                        answers = [{'key':item['key'],'selected':[item['options'][1]['key']]} for item in question['items']]
                        delegated = '用户已明确委托你回答这一次原问题' in text
                        argv = [exe,'brain','answer' if delegated else 'propose-answer','--target',json.dumps(question['target']),'--answers-json',json.dumps(answers)]
                        if not delegated: argv += ['--reason','Use Green for this question']
                        execute(argv,'question-ui-brain')
            except Exception as error:
                with open(root + '/question-brain-error','a') as f: f.write(str(error) + '\n')
            send({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'end_turn'}})
        elif mid is not None: send({'jsonrpc':'2.0','id':mid,'error':{'code':-32601,'message':'unsupported'}})
    """#
}
