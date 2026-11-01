import Foundation

/// 真实 Claude 的就绪形态：启动即发 SessionStart，并把收到的每行输入记到 `received.txt`。
public enum FakeReadyAgentShell {
    public static func install() throws -> FakeStartupAgentShell.Install {
        let install = try FakeStartupAgentShell.install()
        let script = install.root.appendingPathComponent("bin/claude")
        try Data(claude.utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return install
    }

    public static func received(_ install: FakeStartupAgentShell.Install) -> String {
        (try? String(contentsOf: install.root.appendingPathComponent("received.txt"), encoding: .utf8)) ?? ""
    }

    private static let claude = """
    #!/bin/sh
    echo "CLAUDE BANNER"
    /usr/bin/python3 "$NORMA_FAKE_ROOT/bin/send-session-start" Claude
    while IFS= read -r line; do printf '%s\\n' "$line" >> "$NORMA_FAKE_ROOT/received.txt"; done
    """
}
