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
    # Optional real installed helper reproduces upgrades without changing the user's hook config.
    if os.environ.get('NORMA_E2E_STAFF_HOOK_BINARY'):
        forward = [os.environ['NORMA_E2E_STAFF_HOOK_BINARY'], 'forward', '--source', 'claude']
    sid = 'staff-' + str(node)
    def record(name, value):
        with open(os.path.join(root, name), 'a') as f: f.write(json.dumps(value) + '\n')
    def hook(event, **extra):
        payload = dict(hook_event_name=event, session_id=sid, cwd=os.getcwd(), prompt_id='staff-turn', **extra)
        command = forward + (['--request-question'] if event == 'PreToolUse' and extra.get('tool_name') == 'AskUserQuestion' else [])
        result = subprocess.run(command, input=json.dumps(payload), capture_output=True, text=True, timeout=140 if event == 'PreToolUse' else 10)
        if result.returncode: raise RuntimeError(result.stderr)
        return json.loads(result.stdout) if result.stdout.strip() else {}
    record('starts.jsonl', dict(node=node, dispatch=dispatch, pid=os.getpid(), helper=forward[0]))
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
            if os.environ.get('NORMA_E2E_STAFF_QUESTIONS') == '1':
                items = [{'question':'Record acceptance notes?', 'header':'Notes', 'options':[{'label':'Yes','description':'Include notes in the result'}, {'label':'No','description':'Only the artifact'}]}]
                reply = hook('PreToolUse', tool_name='AskUserQuestion', tool_use_id='staff-native-question', tool_input={'questions':items})
                record('worker-question-replies.jsonl', dict(node=node, reply=reply))
                updated = reply.get('hookSpecificOutput',{}).get('updatedInput')
                if not isinstance(updated,dict) or not updated.get('answers'): raise RuntimeError('Missing original Worker question answer')
                hook('PostToolUse', tool_name='AskUserQuestion', tool_use_id='staff-native-question', tool_input=updated, tool_response=updated)
            with open('STAFF-RESULT.md', 'w') as f: f.write('# STAFF RESULT\n\nAcceptance artifact from the dispatched Worker.\n')
            print('\r\nSTAFF RESULT ready\r', flush=True)
            # 后续任务带 STAFF-HOLD 时停在这里，场景放行文件出现前不结束，用来点“撤回”。
            while 'STAFF-HOLD' in task and not os.path.exists(os.path.join(root, 'release-worker')): time.sleep(.05)
            final = 'STAFF RESULT: 需要你确认验收；文件 STAFF-RESULT.md 已生成。'
            if os.environ.get('NORMA_E2E_STAFF_LONG_RESULT') == '1':
                final = 'STAFF RESULT: `tank.html` 已写好，JS 语法检查通过，但没在浏览器里实际玩过一局。\n\n写入时工具提示该文件已存在，我是直接覆盖的，没有还原内容。\n\n- **操作**：方向键或 WASD 移动，空格开炮。\n- **地图**：砖墙能打碎，钢墙打不碎。\n- **其他**：玩家同一时间只能有一发炮弹在飞。\n\n没部署，没推送。'
            hook('Stop', last_assistant_message=final)
    """#
}
