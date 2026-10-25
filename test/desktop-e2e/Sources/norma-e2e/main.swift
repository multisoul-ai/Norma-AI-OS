import E2EKit
import Foundation
import Scenarios

@main
struct Main {
    static func main() async {
        let arguments = CommandLine.arguments
        if arguments.count == 3, arguments[1] == "recognize" {
            do {
                try writeAccurateRecognition(pngPath: arguments[2])
                exit(0)
            } catch {
                fputs("\(error)\n", stderr)
                exit(3)
            }
        }
        if arguments.count >= 2, arguments[1] == "list" {
            let selector = arguments.count >= 3 ? arguments[2] : nil
            let code = await MainActor.run { () -> Int32 in
                if let selector {
                    guard let names = ScenarioCatalog.names(matching: selector) else {
                        fputs("未知模块或场景 \(selector)。\n", stderr)
                        return 2
                    }
                    names.forEach { print($0) }
                    return 0
                }
                for module in ScenarioCatalog.modules {
                    print(module.name)
                    module.scenarioNames.forEach { print("  \($0)") }
                }
                return 0
            }
            exit(code)
        }
        guard arguments.count == 5, ["run", "visual"].contains(arguments[1]) else {
            fputs("用法：norma-e2e list [module|scenario]\n      norma-e2e run|visual <scenario> <app-binary> <artifact-root>\n", stderr)
            exit(2)
        }
        let scenario = arguments[2]
        let visual = arguments[1] == "visual"
        let supported = await MainActor.run { ScenarioCatalog.modules.flatMap(\.visualScenarioNames).contains(scenario) }
        if visual && !supported {
            fputs("场景 \(scenario) 尚未接入模型看图，不能隐式退回 OCR。\n", stderr)
            exit(2)
        }
        let session = Session(scenario: scenario, appPath: arguments[3], artifactRoot: arguments[4], visualReview: visual)
        do {
            try Preflight.check(appPath: session.appPath, artifactRoot: session.artifactRoot)
            try session.begin()
            try await ScenarioCatalog.run(scenario, session)
            try session.finishSuccess()
        } catch {
            session.finishFailure(error)
            exit((error as? E2EError)?.kind == .env ? 3 : 1)
        }
    }
}
