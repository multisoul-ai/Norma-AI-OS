import AVFoundation
import SwiftUI
import NormaCore

struct IdeaTextEditorView: View {
    @Bindable var model: IdeaLibraryModel
    let editing: Idea?
    let onSaved: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var bodyText: String
    @State private var audioURL: URL?
    @State private var player: AVAudioPlayer?
    @State private var retryingTranscription = false
    @State private var bodyChanged = false

    private var currentIdea: Idea? {
        guard let editing else { return nil }
        return model.ideas.first { $0.id == editing.id } ?? editing
    }

    init(model: IdeaLibraryModel, editing: Idea?, onSaved: @escaping () -> Void) {
        self.model = model
        self.editing = editing
        self.onSaved = onSaved
        _bodyText = State(initialValue: editing?.body ?? "")
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Text(editing == nil ? "记下现在的想法" : "编辑这条想法")
                        .font(.largeTitle.bold()).tracking(-1)
                        .accessibilityAddTraits(.isHeader)
                    Text("这里的内容只保存在你的 iPhone，不会开始执行。")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    if editing?.kind == .voice {
                        Button(action: togglePlayback) {
                            Label(player?.isPlaying == true ? "停止播放录音" : "播放原始录音", systemImage: "play.circle.fill")
                                .frame(maxWidth: .infinity, minHeight: 48)
                        }
                        .buttonStyle(IdeaPressStyle())
                        .disabled(audioURL == nil)
                        .accessibilityIdentifier("idea-play-recording")
                        if currentIdea?.transcriptionState == .pending || currentIdea?.transcriptionState == .failed {
                            Button {
                                guard let editing else { return }
                                retryingTranscription = true
                                Task {
                                    let succeeded = await model.retryTranscription(for: editing)
                                    if succeeded && !bodyChanged,
                                       let updated = model.ideas.first(where: { $0.id == editing.id }) {
                                        bodyText = updated.body
                                    }
                                    retryingTranscription = false
                                }
                            } label: {
                                Label(retryingTranscription ? "正在转写…" : "重新转写", systemImage: "waveform")
                                    .frame(maxWidth: .infinity, minHeight: 48)
                            }
                            .buttonStyle(IdeaPressStyle())
                            .disabled(retryingTranscription || audioURL == nil)
                            .accessibilityIdentifier("idea-retry-transcription")
                        }
                    }
                    ZStack(alignment: .topLeading) {
                        RoundedRectangle(cornerRadius: 24).fill(.white)
                        if bodyText.isEmpty {
                            Text("写下你想记住的事…")
                                .foregroundStyle(IdeaPalette.faint)
                                .padding(.horizontal, 20).padding(.top, 19)
                                .allowsHitTesting(false)
                        }
                        TextEditor(text: Binding(get: { bodyText }, set: { bodyText = $0; bodyChanged = true }))
                            .font(.body)
                            .scrollContentBackground(.hidden)
                            .padding(.horizontal, 14).padding(.vertical, 12)
                            .accessibilityIdentifier("idea-body-editor")
                    }
                    .frame(minHeight: 250)
                    Text("可以稍后继续修改。保存不会创建工作空间或任务。")
                        .font(.caption).foregroundStyle(IdeaPalette.secondary)
                }
                .padding(.horizontal, 20).padding(.top, 25)
            }
            .background(IdeaPalette.background)
            .navigationTitle(editing == nil ? "文字记录想法" : "编辑想法")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消") { dismiss() }.accessibilityIdentifier("idea-editor-cancel")
                }
            }
            .safeAreaInset(edge: .bottom) {
                Button {
                    Task { if await model.saveText(bodyText, editing: editing) { onSaved() } }
                } label: {
                    Label("保存到想法库", systemImage: "square.and.arrow.down")
                        .font(.headline)
                        .frame(maxWidth: .infinity, minHeight: 56)
                        .foregroundStyle(.white)
                        .background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 17))
                }
                .buttonStyle(IdeaPressStyle())
                .disabled(model.saving || (editing?.kind != .voice && bodyText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty))
                .accessibilityIdentifier("save-idea")
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 7)
                .background(.regularMaterial)
            }
        }
        .tint(IdeaPalette.blue)
        .task { if let editing, editing.kind == .voice { audioURL = await model.recordingURL(for: editing) } }
        .onDisappear { stopPlayback() }
        .alert("无法保存想法", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("好") { model.error = nil }
        } message: { Text(model.error ?? "") }
    }

    private func togglePlayback() {
        if player?.isPlaying == true {
            stopPlayback()
            return
        }
        guard let audioURL else { return }
        let playback: AVAudioPlayer
        var activated = false
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playback, mode: .default)
            try session.setActive(true)
            activated = true
            playback = try AVAudioPlayer(contentsOf: audioURL)
            guard playback.play() else { throw IdeaStoreError.invalidRecording }
        } catch {
            if activated { try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation) }
            model.error = "无法播放录音，请稍后重试。"
            return
        }
        player = playback
        Task { @MainActor in
            while player === playback && playback.isPlaying {
                try? await Task.sleep(for: .milliseconds(200))
            }
            if player === playback { stopPlayback() }
        }
    }

    private func stopPlayback() {
        guard let playback = player else { return }
        playback.stop()
        player = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }
}
