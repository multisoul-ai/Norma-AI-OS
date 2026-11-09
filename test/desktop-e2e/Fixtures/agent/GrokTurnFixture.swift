import Foundation

/// 脚本化 Grok：经生产 `multisoul-agent-hook forward --source grok` 上报，payload 形状取自真实 Grok
/// （camelCase 主键 + snake 别名、promptId、reason=end_turn、backgroundTasks）。不启动真实模型。
/// 假 HOME 里的 Grok hook 配置由同一 hook 二进制的 `install --target grok` 写入，与生产一致。
///
/// 触发文件（写到 root 下）驱动三段：
///   `turn-1`：问答回合。PreToolUse(ask_user_question) 走 `--request-question`，随后 PostToolUse
///             UserAnswered（终端作答）收题，再发 Stop(p1)。
///   `turn-2`：普通回合 "hi"：UserPromptSubmit(p2) → Stop(p2)。
///   `clear`：/clear 换到新会话，再送旧会话迟到的 Stop(p2)，即会被门禁拒收的那条。
public enum GrokTurnFixture {
    public static let readyBanner = "GROK TURN READY"

    public struct Install {
        public let root: URL
        public var shell: URL { root.appendingPathComponent("shell", isDirectory: true) }
        public var home: URL { root.appendingPathComponent("home", isDirectory: true) }

        public func trigger(_ name: String) throws {
            try Data().write(to: root.appendingPathComponent(name))
        }

        public func done(_ name: String) -> Bool {
            FileManager.default.fileExists(atPath: root.appendingPathComponent("\(name).done").path)
        }

        public var log: URL { root.appendingPathComponent("hook.log") }
    }

    public static func install(hookBinary: String) throws -> Install {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-grok-turn-\(UUID().uuidString)", isDirectory: true)
        let install = Install(root: root)
        let bin = install.shell.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: install.home, withIntermediateDirectories: true)
        let zshrc = """
        export PATH=\"\(bin.path):$PATH\"
        export NORMA_GROK_TURN_ROOT=\"\(root.path)\"
        export NORMA_HOOK_BINARY=\"\(hookBinary)\"
        """
        try Data(zshrc.utf8).write(to: install.shell.appendingPathComponent(".zshrc"))
        let path = bin.appendingPathComponent("grok")
        try Data(agent.utf8).write(to: path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)

        let process = Process()
        process.executableURL = URL(fileURLWithPath: hookBinary)
        process.arguments = ["install", "--target", "grok", "--command-path", hookBinary]
        var environment = ProcessInfo.processInfo.environment
        environment["HOME"] = install.home.path
        environment.removeValue(forKey: "GROK_HOME")
        process.environment = environment
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw NSError(domain: "GrokTurnFixture", code: Int(process.terminationStatus),
                          userInfo: [NSLocalizedDescriptionKey: "hook install --target grok 失败"])
        }
        return install
    }

    private static let agent = #"""
    #!/usr/bin/env python3
    import json, os, subprocess, time
    root = os.environ['NORMA_GROK_TURN_ROOT']
    hook = os.environ['NORMA_HOOK_BINARY']
    if not os.environ.get('NORMA_CONTROL_RUN_ID'):
        print('grok missing NORMA_CONTROL_RUN_ID', flush=True)
        raise SystemExit(1)
    sid_a = 'grok-e2e-a-' + os.environ['NORMA_CONTROL_RUN_ID'][:8]
    sid_b = 'grok-e2e-b-' + os.environ['NORMA_CONTROL_RUN_ID'][:8]

    def payload(event, sid, **extra):
        body = {'hookEventName': event.lower(), 'hook_event_name': event,
                'sessionId': sid, 'session_id': sid, 'cwd': os.getcwd()}
        body.update(extra)
        return body

    def log(line):
        with open(root + '/hook.log', 'a') as handle:
            handle.write(line + '\n')

    def forward(body, extra=(), timeout=20):
        result = subprocess.run([hook, 'forward', '--source', 'grok'] + list(extra),
            input=json.dumps(body), capture_output=True, text=True, timeout=timeout)
        log('%s rc=%s err=%s' % (body['hook_event_name'], result.returncode, result.stderr.strip()))

    def stop(sid, prompt_id):
        forward(payload('Stop', sid, promptId=prompt_id, reason='end_turn',
            stopHookActive=False, stop_hook_active=False, backgroundTasks=[], sessionCrons=[]))

    def wait(name):
        flag = root + '/' + name
        while not os.path.exists(flag):
            time.sleep(0.1)
        os.remove(flag)

    def done(name):
        open(root + '/' + name + '.done', 'w').close()
        print('GROK ' + name.upper() + ' DONE', flush=True)

    forward(payload('SessionStart', sid_a, source='new'))
    print('GROK TURN READY', flush=True)

    wait('turn-1')
    forward(payload('UserPromptSubmit', sid_a, promptId='p1', prompt='测试一下问答工具'))
    question = payload('PreToolUse', sid_a, promptId='p1', tool_name='ask_user_question',
        toolName='ask_user_question', tool_use_id='grok-q-1',
        tool_input={'questions': [{'question': 'Spike color?', 'header': 'Color', 'options': [
            {'label': 'Blue', 'description': 'Blue color'},
            {'label': 'Green', 'description': 'Green color'}]}]})
    forward(question)
    held = subprocess.Popen([hook, 'forward', '--source', 'grok', '--request-question'],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    held.stdin.write(json.dumps(question))
    held.stdin.close()
    print('GROK QUESTION ASKED', flush=True)
    wait('answer-1')
    answered = dict(question, hookEventName='post_tool_use', hook_event_name='PostToolUse',
        tool_response={'type': 'AskUserQuestion', 'UserAnswered': {'message': 'Blue'}})
    forward(answered)
    try:
        held.wait(timeout=20)
    except subprocess.TimeoutExpired:
        held.kill()
    log('question hook rc=%s' % held.returncode)
    stop(sid_a, 'p1')
    done('turn-1')

    wait('turn-2')
    forward(payload('UserPromptSubmit', sid_a, promptId='p2', prompt='hi'))
    time.sleep(1)
    stop(sid_a, 'p2')
    done('turn-2')

    wait('clear')
    forward(payload('SessionStart', sid_b, source='clear'))
    time.sleep(0.5)
    stop(sid_a, 'p2')
    done('clear')
    time.sleep(3600)
    """#
}
