import Foundation

/// 脚本化 Grok / Kimi / ZCode / InfCode。进程调用生产 `norma-hook`，不启动真实模型。
public enum HookQuestionFixture {
    public static func install(hookBinary: String) throws -> (shell: URL, root: URL) {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-hook-questions-\(UUID().uuidString)", isDirectory: true)
        let shell = root.appendingPathComponent("shell", isDirectory: true)
        let bin = shell.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let zshrc = """
        export PATH=\"\(bin.path):$PATH\"
        export NORMA_HOOK_QUESTION_ROOT=\"\(root.path)\"
        export NORMA_HOOK_BINARY=\"\(hookBinary)\"
        unset INFCODE_BIN
        """
        try Data(zshrc.utf8).write(to: shell.appendingPathComponent(".zshrc"))
        for name in ["grok", "kimi", "zcode", "infcode"] {
            let path = bin.appendingPathComponent(name)
            try Data(agent.utf8).write(to: path)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        }
        return (shell, root)
    }

    /// 每张卡片一个进程。`argv[0]` 决定 provider。触发文件 `<provider>-<cycle>` 的内容是 mode。
    private static let agent = #"""
    #!/usr/bin/env python3
    import json, os, subprocess, sys, time
    root = os.environ['NORMA_HOOK_QUESTION_ROOT']
    hook = os.environ['NORMA_HOOK_BINARY']
    provider = os.path.basename(sys.argv[0])
    tool, id_key, kind = {
        'grok': ('ask_user_question', 'tool_use_id', 'deny'),
        'kimi': ('AskUserQuestion', 'tool_call_id', 'kimi'),
        'zcode': ('AskUserQuestion', 'tool_use_id', 'allow'),
        'infcode': ('question', 'tool_use_id', 'infcode'),
    }[provider]
    run_id = os.environ.get('NORMA_CONTROL_RUN_ID', '')
    if not run_id:
        print(provider + ' missing NORMA_CONTROL_RUN_ID', flush=True)
        sys.exit(1)
    sid = 'hook-ui-' + run_id
    with open(root + '/' + provider + '.ready', 'w') as handle:
        handle.write(sid)
    print(provider + ' question fixture ready', flush=True)

    def questions(cycle):
        prompt = 'Spike color %d?' % cycle
        return [{
            'question': prompt, 'header': 'Color',
            'options': [
                {'label': 'Blue', 'description': 'Blue color'},
                {'label': 'Green', 'description': 'Green color'},
            ],
        }]

    def pre(cycle):
        items = questions(cycle)
        body = {
            'hook_event_name': 'PreToolUse', 'session_id': sid, 'tool_name': tool,
            id_key: '%s-tool-%d' % (provider, cycle),
            'tool_input': {'questions': items},
        }
        return body

    def post_for(body, stdout_obj):
        items = body['tool_input']['questions']
        posted = {
            'hook_event_name': 'PostToolUse', 'session_id': sid, 'tool_name': tool,
            id_key: body[id_key], 'tool_input': {'questions': items},
        }
        if kind == 'deny':
            posted['tool_response'] = {'type': 'AskUserQuestion', 'UserAnswered': {'message': 'Blue'}}
        elif kind == 'allow':
            updated = None
            if isinstance(stdout_obj, dict):
                updated = stdout_obj.get('hookSpecificOutput', {}).get('updatedInput')
            if not isinstance(updated, dict):
                updated = {'questions': items, 'answers': {items[0]['question']: 'Blue'}}
            posted['tool_input'] = updated
            posted['tool_response'] = updated
        else:
            posted['tool_response'] = {'questions': items}
        return posted

    def forward(extra, payload, timeout):
        return subprocess.run([hook, 'forward', '--source', provider] + extra,
            input=json.dumps(payload), capture_output=True, text=True, timeout=timeout)

    def record(cycle, mode, result, elapsed):
        meta = {
            'provider': provider, 'cycle': cycle, 'mode': mode, 'elapsed': elapsed,
            'returncode': result.returncode, 'stdout': result.stdout or '', 'stderr': result.stderr or '',
        }
        with open(root + '/%s-%d.json' % (provider, cycle), 'w') as handle:
            json.dump(meta, handle)
        with open(root + '/hook.log', 'a') as handle:
            handle.write('%s %d %s\n%s\n%s\n' % (provider, cycle, mode, meta['stderr'], meta['stdout']))
        print('ANSWER_RECORDED %s %d %s' % (provider, cycle, mode), flush=True)

    def send_post(body, stdout_obj):
        forward([], post_for(body, stdout_obj), 20)

    cycle = 0
    while True:
        cycle += 1
        flag = root + '/%s-%d' % (provider, cycle)
        while not os.path.exists(flag):
            time.sleep(0.1)
        mode = open(flag).read().strip() or 'choice'
        os.remove(flag)
        body = pre(cycle)
        started = time.time()
        if mode == 'terminal':
            proc = subprocess.Popen(
                [hook, 'forward', '--source', provider, '--request-question'],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            proc.stdin.write(json.dumps(body))
            proc.stdin.close()
            close_flag = root + '/%s-%d-close' % (provider, cycle)
            while not os.path.exists(close_flag) and proc.poll() is None:
                time.sleep(0.1)
            if os.path.exists(close_flag):
                os.remove(close_flag)
            send_post(body, None)
            try:
                out, err = proc.communicate(timeout=20)
            except subprocess.TimeoutExpired:
                proc.kill()
                out, err = proc.communicate()
            record(cycle, mode, subprocess.CompletedProcess(args=[], returncode=proc.returncode or 0, stdout=out, stderr=err), time.time() - started)
            continue
        if mode == 'native':
            try:
                result = forward(['--request-question'], body, 20)
            except subprocess.TimeoutExpired as error:
                record(cycle, mode, subprocess.CompletedProcess(args=[], returncode=124, stdout=error.stdout or '', stderr=(error.stderr or '') + 'native hook still blocked'), time.time() - started)
                continue
            record(cycle, mode, result, time.time() - started)
            close_flag = root + '/%s-%d-close' % (provider, cycle)
            while not os.path.exists(close_flag):
                time.sleep(0.1)
            os.remove(close_flag)
            send_post(body, None)
            continue
        try:
            result = forward(['--request-question'], body, 130)
        except subprocess.TimeoutExpired as error:
            record(cycle, mode, subprocess.CompletedProcess(args=[], returncode=124, stdout=error.stdout or '', stderr=error.stderr or 'timeout'), time.time() - started)
            continue
        stdout_obj = None
        if result.stdout.strip():
            try:
                stdout_obj = json.loads(result.stdout)
            except json.JSONDecodeError:
                stdout_obj = None
        if kind == 'allow':
            send_post(body, stdout_obj)
        record(cycle, mode, result, time.time() - started)
    """#
}
