import Foundation

/// 确定性 Agent 经生产观察 Hook 显示原生菜单、消费数字键；不执行请求中的命令。
public enum BrainApprovalFixture {
    public static func install() throws -> (shell: URL, brain: URL) {
        let brain = try BrainFixture.installScriptedBrainDirectory(mode: .propose)
        let shell = URL(fileURLWithPath: "/private/tmp").appendingPathComponent("norma-approval-ui-\(UUID().uuidString)")
        let bin = shell.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\n".utf8).write(to: shell.appendingPathComponent(".zshrc"))
        let path = bin.appendingPathComponent("claude")
        try Data(agent.utf8).write(to: path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        let adapter = brain.appendingPathComponent("adapters/node_modules/.bin/claude-agent-acp")
        try Data(quietAdapter.utf8).write(to: adapter)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: adapter.path)
        return (shell, brain)
    }

    private static let quietAdapter = #"""
    #!/usr/bin/env python3
    import json, sys
    for line in sys.stdin:
        message = json.loads(line)
        mid = message.get('id')
        method = message.get('method')
        if mid is None: continue
        result = {}
        if method == 'initialize': result = {'protocolVersion':1,'agentCapabilities':{}}
        elif method == 'session/new': result = {'sessionId':'approval-ui-brain'}
        elif method == 'session/prompt': result = {'stopReason':'end_turn'}
        print(json.dumps({'jsonrpc':'2.0','id':mid,'result':result}), flush=True)
    """#

    private static let agent = #"""
    #!/usr/bin/env python3
    import json, os, shlex, subprocess, sys, time, tty
    root = os.environ['NORMA_BRAIN_DIR']
    settings = json.loads(sys.argv[sys.argv.index('--settings') + 1])
    forward = settings['hooks']['SessionStart'][0]['hooks'][0]['command']
    sid = 'approval-ui-' + os.environ['NORMA_CONTROL_RUN_ID']
    cwd = os.getcwd()
    directory = os.environ['CLAUDE_CONFIG_DIR'] + '/projects/e2e'
    os.makedirs(directory, exist_ok=True)
    transcript = directory + '/' + sid + '.jsonl'
    open(transcript, 'w').close()
    def hook(name, raw, turn=None):
        raw = dict(raw, hook_event_name=name, session_id=sid, cwd=cwd, transcript_path=transcript)
        if turn: raw['prompt_id'] = turn
        result = subprocess.run(shlex.split(forward), input=json.dumps(raw), capture_output=True, text=True, timeout=10)
        with open(root + '/approval-hook.log', 'a') as f: f.write(name + ': ' + result.stderr + '\n')
        if result.returncode: raise RuntimeError(result.stderr)
    def append(turn, content, uuid):
        with open(transcript, 'a') as f:
            f.write(json.dumps({'type':'user','uuid':uuid,'sessionId':sid,'cwd':cwd,'promptId':turn,
                'message':{'role':'user','content':content}}) + '\n')
    def show(text):
        sys.stdout.write('\x1b[2J\x1b[H' + text.replace('\n','\r\n'))
        sys.stdout.flush()
    hook('SessionStart', {'source':'startup'})
    tty.setraw(sys.stdin.fileno())
    show('Approval fixture ready')
    for cycle in [1, 2]:
        while not os.path.exists(root + '/approval-' + str(cycle)): time.sleep(.1)
        turn = 'approval-turn-' + str(cycle)
        tool = 'approval-ui-tool-' + str(cycle)
        command = 'cargo test -p norma --test brain_decision_copy'
        description = 'Check decision copy'
        call = {'tool_name':'Bash','tool_use_id':tool,'tool_input':{'command':command,'description':description}}
        append(turn, [{'type':'text','text':'Check the decision copy'}], 'prompt-' + str(cycle))
        hook('UserPromptSubmit', {'prompt':'Check the decision copy'}, turn)
        hook('PreToolUse', call, turn)
        hook('PermissionRequest', call, turn)
        show('Bash command\n  ' + command + '\n  ' + description + '\nDo you want to proceed?\n  1. Yes\n  2. No\nEsc to cancel · Tab to amend · ctrl+e to explain')
        key = sys.stdin.read(1)
        if key not in ('1','2'): raise RuntimeError('unexpected native choice: ' + repr(key))
        behavior = 'allow' if key == '1' else 'deny'
        with open(root + '/approval-decisions.jsonl', 'a') as f:
            f.write(json.dumps({'cycle':cycle,'tool_use_id':tool,'behavior':behavior,'key':key}) + '\n')
        if key == '1':
            append(turn, [{'type':'tool_result','tool_use_id':tool,'content':'Checked'}], 'result-' + str(cycle))
            hook('PostToolUse', dict(call, tool_response={'observed_only':True}), turn)
        else:
            rejected = "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed."
            append(turn, [{'type':'tool_result','tool_use_id':tool,'is_error':True,'content':rejected}], 'result-' + str(cycle))
            append(turn, [{'type':'text','text':'[Request interrupted by user for tool use]'}], 'interrupted-' + str(cycle))
        show('Decision received ' + str(cycle))
    while True: time.sleep(1)
    """#
}
