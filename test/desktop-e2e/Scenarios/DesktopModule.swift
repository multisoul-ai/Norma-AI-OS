import E2EKit

/// 模块清单。用具体类型保存，避免 `any Protocol.Type` 让当前 Swift 编译器崩溃。
public struct DesktopModule {
    public let name: String
    public let scenarioNames: [String]
    public let visualScenarioNames: [String]
    public let run: @MainActor (String, Session) async throws -> Bool

    public init(
        name: String,
        scenarioNames: [String],
        visualScenarioNames: [String] = [],
        run: @escaping @MainActor (String, Session) async throws -> Bool
    ) {
        self.name = name
        self.scenarioNames = scenarioNames
        self.visualScenarioNames = visualScenarioNames
        self.run = run
    }
}
