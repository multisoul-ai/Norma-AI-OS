import AppKit
import CoreGraphics
import CoreText
import E2EKit
import Foundation

/// 真实外部文件、终端输出脚本，以及透明记录后交给 /usr/bin/open 的系统打开观察器。
public struct ExternalFilesFixture {
    public let root: URL
    public var home: URL { root.appendingPathComponent("home") }
    public var external: URL { root.appendingPathComponent("外部 files") }
    public var script: URL { root.appendingPathComponent("show.sh") }
    public var receipts: URL { root.appendingPathComponent("open.jsonl") }
    public static let inlineFiles = ["note.txt", "note.md", "page.html", "PatternE2E.png", "ClipE2E.mp4"]
    public static let systemFiles = ["ReportE2E.pdf", "OfficeE2E.docx"]

    public static func install() throws -> Self {
        let fixture = Self(root: URL(fileURLWithPath: "/private/tmp/nex-\(UUID().uuidString.prefix(8))"))
        try FileManager.default.createDirectory(at: fixture.external, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: fixture.home, withIntermediateDirectories: true)
        try Data("unset HISTFILE\nSAVEHIST=0\nHISTSIZE=0\nPS1=\"e2e> \"\n".utf8).write(to: fixture.home.appendingPathComponent(".zshrc"))
        try Data("EXTERNAL TEXT DOCUMENT\n项目外文档正文".utf8).write(to: fixture.external.appendingPathComponent("note.txt"))
        try Data("# EXTERNAL MARKDOWN\n\nExternal document body.".utf8).write(to: fixture.external.appendingPathComponent("note.md"))
        try Data("<h1>EXTERNAL HTML</h1>".utf8).write(to: fixture.external.appendingPathComponent("page.html"))
        try ContentMediaFixture.install(fixture.external)
        let document = NSAttributedString(string: "EXTERNAL OFFICE DOCUMENT\n项目外 Word 文档")
        let docx = try document.data(from: NSRange(location: 0, length: document.length),
            documentAttributes: [.documentType: NSAttributedString.DocumentType.officeOpenXML])
        try docx.write(to: fixture.external.appendingPathComponent("OfficeE2E.docx"))
        try fixture.writePDF()
        let lines = (inlineFiles + systemFiles).map { "printf '%s\\n' '\u{0060}\(fixture.external.appendingPathComponent($0).path)\u{0060}'" }
        try Data((["#!/bin/sh", "clear", "printf 'EXTERNAL PATH LINKS\\n'"] + lines).joined(separator: "\n").utf8).write(to: fixture.script)
        let bin = fixture.root.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let observer = bin.appendingPathComponent("open")
        try Data("""
        #!/usr/bin/python3
        import json, os, subprocess, sys, time
        result = subprocess.run(['/usr/bin/open', *sys.argv[1:]])
        with open(os.environ['NORMA_EXTERNAL_OPEN_LOG'], 'a') as stream:
            stream.write(json.dumps({'args': sys.argv[1:], 'status': result.returncode, 'time': time.time()}) + '\\n')
        sys.exit(result.returncode)
        """.utf8).write(to: observer)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: observer.path)
        return fixture
    }

    public func seed(database: URL, workspace: String) throws {
        let project = root.appendingPathComponent(workspace)
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        try Catalog.seedWorkspace(database: database, name: workspace)
        try Catalog.seedArea(database: database, workspaceName: workspace, root: project, displayName: "Source Project")
        try Catalog.suppressFeatureGuide(database: database)
    }

    private func writePDF() throws {
        var bounds = CGRect(x: 0, y: 0, width: 612, height: 792)
        guard let context = CGContext(external.appendingPathComponent("ReportE2E.pdf") as CFURL, mediaBox: &bounds, nil) else {
            throw E2EError.env("无法生成 PDF 夹具")
        }
        context.beginPDFPage(nil)
        context.textPosition = CGPoint(x: 60, y: 700)
        let text = NSAttributedString(string: "EXTERNAL PDF DOCUMENT", attributes: [.font: NSFont.systemFont(ofSize: 22)])
        CTLineDraw(CTLineCreateWithAttributedString(text), context)
        context.endPDFPage()
        context.closePDF()
    }

    /// 系统应用边界：精确路径、真实 open 成功回执、默认应用实际运行；不模拟系统阅读器。
    @MainActor
    public func waitForSystemOpen(_ file: String, after count: Int) async throws -> String {
        let target = external.appendingPathComponent(file)
        guard let application = NSWorkspace.shared.urlForApplication(toOpen: target) else {
            throw E2EError.env("没有可打开 \(file) 的系统应用")
        }
        let deadline = Date().addingTimeInterval(20)
        while Date() < deadline {
            let rows = receiptRows()
            if rows.count > count,
               let args = rows.last?["args"] as? [String], args == [target.path],
               let status = rows.last?["status"] as? Int {
                guard status == 0 else { throw E2EError.product("真实 /usr/bin/open 失败：\(status)，\(file)") }
                if let running = NSWorkspace.shared.runningApplications.first(where: { $0.bundleURL == application }) {
                    return "真实 open 精确收到 \(target.path)，exit=0；默认应用 \(application.lastPathComponent)，PID \(running.processIdentifier) 已运行"
                }
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("点击外部 \(file) 后未收到准确的系统打开回执及运行应用")
    }

    public func receiptRows() -> [[String: Any]] {
        let text = (try? String(contentsOf: receipts, encoding: .utf8)) ?? ""
        return text.split(separator: "\n").compactMap { line in
            (try? JSONSerialization.jsonObject(with: Data(line.utf8))) as? [String: Any]
        }
    }
}
