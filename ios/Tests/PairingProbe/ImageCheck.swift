import Foundation
import NormaCore

enum ImageCheck {
    @MainActor static func run(client: MobileClient, machine: PairedMachine) async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_TEST_IMAGE"] else { throw ChatImageError.invalid }
        let bytes = try Data(contentsOf: URL(fileURLWithPath: path))
        let catalog = try await client.snapshot(machine)
        guard let workspace = catalog.projects.first?.id else { throw MobileError.invalidResponse }
        let snapshot = try await client.snapshot(machine, workspaceID: workspace)
        print("IMAGE_CARD_STATE \(snapshot.projects.first?.cards?.first?.state ?? "missing")")
        guard let project = snapshot.projects.first, let route = project.routeID, let card = project.cards?.first,
              card.state == "ready", card.runID != nil else { throw MobileError.invalidResponse }
        let saved = try await client.uploadImage(bytes, to: machine, workspaceID: workspace, revision: snapshot.grant.revision)
        print("IMAGE_UPLOAD_OK \(card.profile)")
        let downloaded = try await client.chatImage(saved.id, on: machine, workspaceID: workspace)
        guard downloaded == bytes else { throw ChatImageError.invalid }
        //The expected code never enters the prompt; it must be read from actual image pixels.
        let prompt = try ChatImageMessage.compose(text: "Read this image with your image-viewing tool. Reply only with the exact TEST- code printed at the top. Do not modify any files.", paths: [saved.path])
        let command = try await client.prepareMessage(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision, card: card, text: prompt)
        _ = try await client.submit(command, to: machine)
        let receipt = try await MessageCheck.receipt(client, machine, command)
        print("IMAGE_RECEIPT \(receipt.state) \(receipt.error ?? "none")")
        guard receipt.state == "succeeded", let result = receipt.result, result.runID == card.runID else { throw MobileError.invalidResponse }
        let completed = try await MessageCheck.completed(client, machine, workspace, route, card.id, "TEST-4829")
        guard completed.runID == card.runID, completed.sessionID == result.sessionID else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_IMAGE_OK \(card.profile)")
    }
}
