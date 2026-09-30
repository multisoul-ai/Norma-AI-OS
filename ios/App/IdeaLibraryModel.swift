import Foundation
import Observation
import NormaCore

@MainActor @Observable final class IdeaLibraryModel {
    private let store: IdeaStore
    let unfiledDirectory: URL
    private(set) var ideas: [Idea] = []
    private(set) var recoverableRecordings: [URL] = []
    private(set) var loaded = false
    private(set) var saving = false
    var error: String?

    init() {
        var directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/Ideas", isDirectory: true)
        #if DEBUG
        if let namespace = ProcessInfo.processInfo.environment["NORMA_IDEA_TEST_NAMESPACE"],
           UUID(uuidString: namespace) != nil {
            directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/UITestIdeas/\(namespace)", isDirectory: true)
        }
        #endif
        store = IdeaStore(databaseURL: directory.appendingPathComponent("ideas.sqlite"),
                          recordingsDirectory: directory.appendingPathComponent("IdeaRecordings", isDirectory: true))
        unfiledDirectory = directory.appendingPathComponent("UnfiledRecordings", isDirectory: true)
    }

    func load() async {
        do {
            ideas = try await store.all()
            recoverableRecordings = (try? FileManager.default.contentsOfDirectory(
                at: unfiledDirectory, includingPropertiesForKeys: [.contentModificationDateKey], options: [.skipsHiddenFiles]))?
                .filter { $0.pathExtension == "caf" }
                .sorted { $0.lastPathComponent > $1.lastPathComponent } ?? []
            loaded = true; error = nil
        }
        catch { self.error = error.localizedDescription }
    }

    func saveText(_ body: String, editing idea: Idea?) async -> Bool {
        guard !saving else { return false }
        saving = true
        defer { saving = false }
        do {
            if let idea { _ = try await store.updateBody(id: idea.id, body: body) }
            else { _ = try await store.createText(body) }
            ideas = try await store.all()
            error = nil
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    func saveVoice(recordingAt url: URL, duration: TimeInterval, transcript: String?,
                   transcriptionComplete: Bool, editedBody: String, userEdited: Bool) async -> Bool {
        guard !saving else { return false }
        saving = true
        defer { saving = false }
        do {
            _ = try await store.createVoice(recordingAt: url, duration: duration,
                                            transcript: transcript, transcriptionComplete: transcriptionComplete,
                                            editedBody: editedBody, userEdited: userEdited)
            ideas = try await store.all()
            error = nil
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    func recordingURL(for idea: Idea) async -> URL? {
        do { return try await store.recordingURL(for: idea.id) }
        catch { self.error = error.localizedDescription; return nil }
    }

    func updateTranscript(for idea: Idea, text: String) async -> Bool {
        do {
            _ = try await store.updateTranscript(id: idea.id, transcript: text)
            ideas = try await store.all()
            error = nil
            return true
        } catch { self.error = error.localizedDescription; return false }
    }

    func retryTranscription(for idea: Idea) async -> Bool {
        do {
            guard let url = try await store.recordingURL(for: idea.id) else {
                throw IdeaStoreError.invalidRecording
            }
            let transcript = try await IdeaVoiceCapture.transcribeFile(at: url)
            _ = try await store.updateTranscript(id: idea.id, transcript: transcript)
            ideas = try await store.all()
            error = nil
            return true
        } catch {
            _ = try? await store.markTranscriptionFailed(id: idea.id)
            if let refreshed = try? await store.all() { ideas = refreshed }
            self.error = error.localizedDescription
            return false
        }
    }

    func removeRecovered(_ url: URL) { recoverableRecordings.removeAll { $0 == url } }
}
