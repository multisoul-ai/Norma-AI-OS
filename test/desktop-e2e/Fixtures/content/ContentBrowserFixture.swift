import E2EKit
import Foundation

/// 独立回环服务器，避免浏览器验收依赖公网或用户登录状态。
public final class ContentBrowserFixture {
    private let process = Process()
    public let address: String
    public let refreshAddress: String

    /// `refreshDirectory` enables a page whose next response can be held without blocking other routes.
    public init(refreshDirectory: URL? = nil) throws {
        let output = Pipe()
        if let refreshDirectory {
            try FileManager.default.createDirectory(at: refreshDirectory, withIntermediateDirectories: true)
            try Data("WEB BEFORE".utf8).write(to: refreshDirectory.appendingPathComponent("body.txt"))
        }
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-u", "-c", #"""
import html, http.server, pathlib, sys, time
state = pathlib.Path(sys.argv[1]) if sys.argv[1] else None
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        second = self.path.startswith('/two')
        title = 'BROWSER SECOND E2E' if second else 'BROWSER FIRST E2E'
        heading = title
        if state and self.path == '/refresh':
            title = 'Loopback'
            with (state / 'requests.log').open('a') as log:
                log.write(self.path + '\n')
            deadline = time.monotonic() + 15
            while (state / 'hold').exists() and time.monotonic() < deadline:
                time.sleep(0.025)
            heading = html.escape((state / 'body.txt').read_text())
        body = '<h1>'+heading+'</h1>'
        body += '<a href="/two">NEXT PAGE E2E</a><p><input aria-label="Session marker" placeholder="Session marker"></p>'
        page = ('<!doctype html><meta charset="utf-8"><title>'+title+'</title><style>body{font:16px -apple-system;padding:20px}input{font-size:18px;width:90%}a{display:block;margin:24px 0}</style>'+body).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        if state and self.path == '/refresh':
            self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(page)))
        self.end_headers()
        self.wfile.write(page)
    def log_message(self, *args): pass
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
print(server.server_port, flush=True)
server.serve_forever()
"""#, refreshDirectory?.path ?? ""]
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        try process.run()
        var port = Data()
        while port.count < 8 {
            let byte = output.fileHandleForReading.readData(ofLength: 1)
            if byte.isEmpty || byte == Data([10]) { break }
            port.append(byte)
        }
        guard let number = Int(String(decoding: port, as: UTF8.self)), number > 0 else {
            if process.isRunning { process.terminate() }
            process.waitUntilExit()
            throw E2EError.env("回环浏览器夹具未返回端口")
        }
        address = "http://127.0.0.1:\(number)/one"
        refreshAddress = "http://127.0.0.1:\(number)/refresh"
    }

    public func stop() {
        if process.isRunning { process.terminate(); process.waitUntilExit() }
    }
}
