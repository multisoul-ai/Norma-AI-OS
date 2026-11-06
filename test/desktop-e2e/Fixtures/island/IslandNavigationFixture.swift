import Foundation

/// 持续运行的脚本 Agent：使用启动参数中的生产 Hook，记录进程身份以验证关窗不重建 PTY。
public enum IslandNavigationFixture {
    public static func install() throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp").appendingPathComponent("norma-island-navigation-\(UUID().uuidString)")
        let bin = root.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\nexport NORMA_ISLAND_FIXTURE=\"\(root.path)\"\n".utf8)
            .write(to: root.appendingPathComponent(".zshrc"))
        let executable = bin.appendingPathComponent("claude")
        try Data(agent.utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
        return root
    }

    private static let agent = #"""
    #!/usr/bin/env python3
    import json, os, shlex, subprocess, sys, time
    root = os.environ['NORMA_ISLAND_FIXTURE']
    settings = json.loads(sys.argv[sys.argv.index('--settings') + 1])
    forward = settings['hooks']['SessionStart'][0]['hooks'][0]['command']
    sid = 'island-navigation-' + os.environ['NORMA_CONTROL_RUN_ID']
    cwd = os.getcwd()
    def hook(name, raw):
        raw = dict(raw, hook_event_name=name, session_id=sid, cwd=cwd, prompt_id='island-live-turn')
        result = subprocess.run(shlex.split(forward), input=json.dumps(raw), capture_output=True, text=True, timeout=10)
        if result.returncode: raise RuntimeError(result.stderr)
    hook('SessionStart', {'source':'startup'})
    hook('UserPromptSubmit', {'prompt':'Keep the island navigation task running'})
    hook('PreToolUse', {'tool_name':'Bash','tool_use_id':'island-live-tool',
        'tool_input':{'command':'sleep 3600','description':'Island navigation live task'}})
    identity = {'pid':os.getpid(), 'node':os.environ['MULTISOUL_NODE_ID'], 'session':sid}
    with open(root + '/starts.jsonl', 'a') as f: f.write(json.dumps(identity) + '\n')
    print('ISLAND LIVE ' + str(identity['pid']) + ' NODE ' + identity['node'], flush=True)
    while True: time.sleep(1)
    """#
}
