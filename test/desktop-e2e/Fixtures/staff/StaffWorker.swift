import Foundation

enum StaffWorker {
    static let script = #"""
    #!/usr/bin/env python3
    import json, os, shlex, subprocess, sys, tty, time
    root = os.environ['NORMA_BRAIN_DIR']
    node = int(os.environ['NORMA_NODE_ID'])
    dispatch = os.environ.get('NORMA_DISPATCH_ID')
    settings = json.loads(sys.argv[sys.argv.index('--settings') + 1])
    forward = shlex.split(settings['hooks']['SessionStart'][0]['hooks'][0]['command'])
    sid = 'staff-' + str(node)
    def record(name, value):
        with open(os.path.join(root, name), 'a') as f: f.write(json.dumps(value) + '\n')
    def hook(event, **extra):
        payload = dict(hook_event_name=event, session_id=sid, cwd=os.getcwd(), prompt_id='staff-turn', **extra)
        result = subprocess.run(forward, input=json.dumps(payload), capture_output=True, text=True, timeout=10)
        if result.returncode: raise RuntimeError(result.stderr)
    record('starts.jsonl', dict(node=node, dispatch=dispatch, pid=os.getpid()))
    tty.setraw(sys.stdin.fileno())
    print('\x1b[?2004hSTAFF WORKER READY\r', flush=True)
    hook('SessionStart', source='startup')
    buffer = b''
    while True:
        data = os.read(0, 4096)
        if not data: break
        buffer += data
        if b'\x1b[201~' in buffer and buffer.endswith(b'\r'):
            task = buffer.replace(b'\x1b[200~', b'').replace(b'\x1b[201~', b'').rstrip(b'\r').decode('utf-8')
            buffer = b''
            record('received.jsonl', dict(node=node, dispatch=dispatch, task=task))
            hook('UserPromptSubmit', prompt=task)
            with open('STAFF-RESULT.md', 'w') as f: f.write('# STAFF RESULT\n\nAcceptance artifact from the dispatched Worker.\n')
            print('\r\nSTAFF RESULT ready\r', flush=True)
            hook('Stop', last_assistant_message='STAFF RESULT: 需要你确认验收；文件 STAFF-RESULT.md 已生成。')
    """#
}
