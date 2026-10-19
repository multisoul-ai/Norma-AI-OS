import E2EKit

/// 只登记模块。某个模块里的场景名单放在该模块的 `<模块>Module.swift`。
@MainActor
public enum ScenarioCatalog {
    public static let modules: [DesktopModule] = [
        WorkspaceModule.manifest,
        CanvasModule.manifest,
        ContentModule.manifest,
        SettingsModule.manifest,
    ]

    public static func names(matching selector: String) -> [String]? {
        if let module = modules.first(where: { $0.name == selector }) {
            return module.scenarioNames
        }
        let known = modules.flatMap(\.scenarioNames)
        return known.contains(selector) ? [selector] : nil
    }

    public static func run(_ name: String, _ session: Session) async throws {
        for module in modules where module.scenarioNames.contains(name) {
            if try await module.run(name, session) { return }
        }
        let known = modules.flatMap(\.scenarioNames).joined(separator: ", ")
        throw E2EError.env("未知场景 \(name)。已注册：\(known)。模块划分见 norma-desktop-e2e 的 modules.md。")
    }
}
