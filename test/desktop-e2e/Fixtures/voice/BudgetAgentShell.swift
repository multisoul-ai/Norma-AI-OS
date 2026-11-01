import Foundation

/// 经生产 Hook 注入长中文会话，不直接改 App 内部状态或用户数据库。
public enum BudgetAgentShell {
    public static func install() throws -> URL {
        let root = try FakeAgentShell.install()
        let script = #"""
        #!/usr/bin/env python3
        import json, os, time, urllib.request
        node = int(os.environ["MULTISOUL_NODE_ID"])
        def post(event, extra):
            raw = dict(extra, hook_event_name=event, session_id="budget-session")
            body = {"node_id": node, "profile": "Claude", "provider_event": event,
                    "session_id": "budget-session", "turn_id": "budget-turn", "raw_json": raw}
            request = urllib.request.Request(os.environ["MULTISOUL_HOOK_ENDPOINT"],
                json.dumps(body).encode(), {"Authorization": "Bearer " + os.environ["MULTISOUL_HOOK_TOKEN"],
                "Content-Type": "application/json"})
            urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3).read()
        time.sleep(2)
        post("SessionStart", {})
        post("UserPromptSubmit", {"prompt": "USER_TAIL:" + "文" * 1200})
        post("Stop", {"last_assistant_message": "AGENT_TAIL:" + "答" * 1200})
        time.sleep(3600)
        """#
        let file = root.appendingPathComponent("bin/claude")
        try Data(script.utf8).write(to: file)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
        return root
    }
}
