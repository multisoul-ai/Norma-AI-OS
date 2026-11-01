import Foundation

/// 原生 Codex 握手期间的交互菜单夹具：假 `codex` 报告已支持的 0.162.1，使应用走原生握手路径。
/// - `app-server`：只创建并监听 socket，让包装脚本放行到 `--remote`。
/// - `--remote`：读取 `$NORMA_FAKE_ROOT/mode`。`menu` 画出「Update available」菜单，上下键移动选择
///   （重绘 `SELECTED n`），回车打印 `CHOSEN n`；`stuck` 每秒刷屏但永不连接中继。
/// 两种模式都不连接中继 socket，因此握手本身只由真实 Codex 的 Rust 集成测试覆盖。
public enum FakeCodexUpdateMenu {
    public struct Install {
        public let root: URL
        public var home: URL { root.appendingPathComponent("home", isDirectory: true) }
        public func setMode(_ mode: String) throws {
            try Data(mode.utf8).write(to: root.appendingPathComponent("mode"))
        }
    }

    public static func install() throws -> Install {
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-fake-codex-menu-\(UUID().uuidString)", isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\nexport NORMA_FAKE_ROOT=\"\(root.path)\"\n".utf8)
            .write(to: root.appendingPathComponent(".zshrc"))
        try write(codex, to: bin.appendingPathComponent("codex"))
        try write(menu, to: bin.appendingPathComponent("update-menu"))
        try write(server, to: bin.appendingPathComponent("fake-server"))
        let install = Install(root: root)
        try install.setMode("menu")
        let hooks = install.home.appendingPathComponent(".codex/hooks.json")
        try FileManager.default.createDirectory(at: hooks.deletingLastPathComponent(), withIntermediateDirectories: true)
        let command = "/usr/local/bin/multisoul-agent-hook forward --source codex"
        let events = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Stop"]
        var map: [String: Any] = [:]
        for event in events { map[event] = [["hooks": [["type": "command", "command": command]]]] }
        try JSONSerialization.data(withJSONObject: ["hooks": map], options: [.sortedKeys]).write(to: hooks)
        return install
    }

    private static func write(_ text: String, to url: URL) throws {
        try Data((text + "\n").utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }

    private static let codex = """
    #!/bin/sh
    case "$*" in
      '--version') echo 'codex-cli 0.162.1';;
      *app-server*) for last; do :; done; exec /usr/bin/python3 "$NORMA_FAKE_ROOT/bin/fake-server" "$last";;
      *--remote*)
        if [ "$(cat "$NORMA_FAKE_ROOT/mode")" = stuck ]; then
          while :; do printf 'STUCK TICK\\r\\n'; sleep 1; done
        fi
        exec /usr/bin/python3 "$NORMA_FAKE_ROOT/bin/update-menu";;
      *) echo 'FALLBACK CODEX'; exec sleep 3600;;
    esac
    """

    private static let server = """
    #!/usr/bin/env python3
    import socket, sys, time
    s = socket.socket(socket.AF_UNIX)
    s.bind(sys.argv[1][len("unix://"):])
    s.listen(1)
    time.sleep(3600)
    """

    private static let menu = #"""
    #!/usr/bin/env python3
    import os, sys, termios, tty, time
    options = ["Update now (runs `npm install -g @openai/codex`)", "Skip", "Skip until next version"]
    selected = 0
    def draw():
        out = "\x1b[2J\x1b[H"
        out += "UPDATE MENU\r\n\r\n"
        for i, text in enumerate(options):
            out += ("> " if i == selected else "  ") + f"{i + 1}. {text}\r\n"
        out += f"\r\nSELECTED {selected + 1}\r\n"
        sys.stdout.write(out); sys.stdout.flush()
    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    tty.setraw(fd)
    try:
        draw()
        while True:
            ch = os.read(fd, 8)
            if ch in (b"\x1b[A", b"\x1bOA"): selected = (selected - 1) % 3; draw()
            elif ch in (b"\x1b[B", b"\x1bOB"): selected = (selected + 1) % 3; draw()
            elif ch in (b"\r", b"\n"):
                sys.stdout.write(f"\r\nCHOSEN {selected + 1}\r\n"); sys.stdout.flush()
                time.sleep(3600)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)
    """#
}
