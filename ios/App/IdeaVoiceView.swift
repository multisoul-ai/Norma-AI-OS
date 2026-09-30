import SwiftUI

struct IdeaVoiceView: View {
    @Bindable var model: IdeaLibraryModel
    let onSaved: () -> Void
    let recoveringURL: URL?
    @StateObject private var capture: IdeaVoiceCapture
    @State private var transcript = ""
    @State private var userEdited = false
    @State private var confirmDiscard = false
    @State private var appeared = false
    @FocusState private var transcriptFocused: Bool
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @ScaledMetric(relativeTo: .body) private var transcriptEditorHeight = 72

    init(model: IdeaLibraryModel, recoveringURL: URL? = nil, onSaved: @escaping () -> Void) {
        self.model = model
        self.recoveringURL = recoveringURL
        self.onSaved = onSaved
        _capture = StateObject(wrappedValue: IdeaVoiceCapture(directory: model.unfiledDirectory))
    }

    var body: some View {
        ZStack {
            IdeaPalette.background.ignoresSafeArea()
            decorations
            VStack(spacing: 0) {
                topBar.padding(.horizontal, 20).padding(.top, 13)
                GeometryReader { geometry in
                    ScrollViewReader { scroll in
                        ScrollView {
                            VStack(spacing: 0) {
                                Spacer(minLength: 12)
                                waveform
                                    .opacity(appeared ? 1 : 0)
                                    .offset(y: appeared || reduceMotion ? 0 : 8)
                                    .animation(.easeOut(duration: reduceMotion ? 0.12 : 0.22), value: appeared)
                                HStack(spacing: 8) {
                                    Circle().fill(capture.phase == .idle ? IdeaPalette.faint : IdeaPalette.green)
                                        .frame(width: 10, height: 10)
                                    Text(capture.phase == .idle ? "等待录音" : capture.phase == .recording ? "正在转写" :
                                         capture.phase == .preparing ? "正在准备录音" :
                                         capture.finalTranscript == nil ? "录音已停止 · 待转写" : "转写已完成")
                                }
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(capture.phase == .idle ? IdeaPalette.secondary : IdeaPalette.green)
                                .padding(.top, 24)
                                .contentTransition(.opacity)
                                .animation(.easeOut(duration: 0.18), value: capture.phase)
                                TimelineView(.periodic(from: .now, by: 1)) { _ in
                                    Text(durationLabel(capture.elapsed))
                                        .font(.title2.weight(.semibold)).monospacedDigit()
                                        .padding(.top, 10)
                                }
                                transcriptCard.padding(.top, 27)
                                    .opacity(appeared ? 1 : 0)
                                    .offset(y: appeared || reduceMotion ? 0 : 10)
                                    .animation(.easeOut(duration: reduceMotion ? 0.12 : 0.22)
                                        .delay(reduceMotion ? 0 : 0.04), value: appeared)
                                if let notice = capture.recognitionNotice {
                                    Text(notice).font(.caption).foregroundStyle(IdeaPalette.secondary)
                                        .frame(maxWidth: .infinity, alignment: .leading).padding(.top, 9)
                                }
                                if let error = capture.error {
                                    Text(error).font(.caption).foregroundStyle(.red)
                                        .frame(maxWidth: .infinity, alignment: .leading).padding(.top, 9)
                                        .accessibilityIdentifier("idea-voice-error")
                                }
                                Spacer(minLength: 12)
                                Button {
                                    if capture.phase == .recording { capture.stop() }
                                    else if capture.phase == .idle { Task { await capture.start() } }
                                } label: {
                                    ZStack {
                                        if capture.phase == .preparing {
                                            ProgressView().tint(.white)
                                        } else {
                                            Image(systemName: capture.phase == .recording ? "pause.fill" : "mic.fill")
                                                .contentTransition(reduceMotion ? .opacity : .symbolEffect(.replace))
                                        }
                                    }
                                        .font(.system(size: 30, weight: .bold))
                                        .foregroundStyle(.white)
                                        .frame(width: 82, height: 82)
                                        .background(Color(red: 1, green: 0.23, blue: 0.39), in: Circle())
                                        .padding(20)
                                        .background(Color.pink.opacity(0.12), in: Circle())
                                        .padding(12)
                                        .background {
                                            Circle().fill(Color.pink.opacity(0.07))
                                                .phaseAnimator(breathes ? [false, true] : [false]) { ring, expanded in
                                                    ring.scaleEffect(expanded ? 1.08 : 1)
                                                        .opacity(expanded ? 0.65 : 1)
                                                } animation: { _ in
                                                    // Ambient readiness/recording cue; taps never wait for this cycle.
                                                    .easeInOut(duration: capture.phase == .recording ? 0.8 : 1.4)
                                                }
                                        }
                                        .animation(.easeOut(duration: 0.2), value: capture.phase)
                                }
                                .buttonStyle(IdeaVoiceRecordStyle())
                                .opacity(appeared ? 1 : 0)
                                .scaleEffect(appeared || reduceMotion ? 1 : 0.96)
                                .animation(.easeOut(duration: reduceMotion ? 0.12 : 0.22)
                                    .delay(reduceMotion ? 0 : 0.08), value: appeared)
                                .disabled(capture.phase == .preparing || capture.phase == .stopped)
                                .accessibilityLabel(recordLabel)
                                .accessibilityIdentifier("idea-voice-record-control")
                                Text(capture.phase == .recording ? "点击停止录音" : capture.phase == .stopped ? "录音已停止" : "点击开始录音")
                                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                    .padding(.top, 5)
                                Spacer(minLength: 18)
                            }
                            .padding(.horizontal, 20)
                            .frame(minHeight: geometry.size.height)
                        }
                        .scrollIndicators(.hidden)
                        .scrollDismissesKeyboard(.interactively)
                        .onChange(of: transcriptFocused) { _, focused in
                            if focused { scroll.scrollTo("transcript-editor", anchor: .top) }
                        }
                        .onChange(of: geometry.size.height) { _, _ in
                            // Re-align after the native keyboard changes the available reading area.
                            if transcriptFocused { scroll.scrollTo("transcript-editor", anchor: .top) }
                        }
                    }
                }
            }
        }
        .foregroundStyle(IdeaPalette.ink)
        .onAppear { appeared = true }
        .onDisappear { appeared = false }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if transcriptFocused {
                HStack {
                    Spacer()
                    Button("完成编辑") { transcriptFocused = false }
                        .font(.body.weight(.semibold)).foregroundStyle(IdeaPalette.blue)
                        .frame(minWidth: 44, minHeight: 44)
                        .accessibilityIdentifier("idea-voice-keyboard-done")
                }
                .padding(.horizontal, 20).padding(.vertical, 4)
                .background(.regularMaterial)
            } else { saveDock }
        }
        .task {
            if let recoveringURL {
                do { try capture.recover(recoveringURL) }
                catch { model.error = "这段录音暂时无法读取，原始文件已保留。" }
            }
        }
        .onChange(of: capture.partial) { _, current in
            if !userEdited { transcript = current }
        }
        .alert(capture.recordingURL == nil ? "放弃这段文字？" : "放弃这段录音？",
               isPresented: $confirmDiscard) {
            Button(capture.recordingURL == nil ? "放弃文字" : "放弃录音", role: .destructive) {
                capture.discard()
                if let recoveringURL { model.removeRecovered(recoveringURL) }
                dismiss()
            }
            Button("继续编辑", role: .cancel) {}
        } message: { Text("尚未保存到想法库。") }
        .alert("无法保存语音想法", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("好") { model.error = nil }
        } message: { Text(model.error ?? "") }
    }

    private var decorations: some View {
        GeometryReader { geometry in
            Circle().fill(Color.yellow.opacity(0.17)).frame(width: 95, height: 95)
                .position(x: 110, y: 3)
            Circle().fill(Color.pink.opacity(0.17)).frame(width: 85, height: 85)
                .position(x: geometry.size.width + 7, y: 110)
            Circle().fill(IdeaPalette.mint.opacity(0.18)).frame(width: 73, height: 73)
                .position(x: geometry.size.width + 4, y: 290)
        }.allowsHitTesting(false).accessibilityHidden(true)
    }

    private var topBar: some View {
        HStack {
            Button { close() } label: {
                Image(systemName: "chevron.left").font(.system(size: 20, weight: .medium))
                    .frame(width: 48, height: 48).background(IdeaPalette.softGray, in: Circle())
            }.accessibilityLabel("返回想法库").accessibilityIdentifier("idea-voice-back")
            Spacer()
            Text("语音记录想法").font(.headline).accessibilityAddTraits(.isHeader)
            Spacer()
            Button { close() } label: {
                Image(systemName: "xmark").font(.system(size: 19, weight: .medium))
                    .frame(width: 48, height: 48).background(IdeaPalette.softGray, in: Circle())
            }.accessibilityLabel("关闭语音记录").accessibilityIdentifier("idea-voice-close")
        }
        .buttonStyle(IdeaPressStyle())
    }

    private var waveform: some View {
        IdeaVoiceWaveform(recording: capture.phase == .recording, level: capture.level, reduceMotion: reduceMotion)
        .accessibilityLabel("录音波形")
        .accessibilityValue(capture.phase == .recording ? "正在录音" : capture.phase == .idle ? "尚未录音" : "录音已停止")
    }

    private var breathes: Bool {
        appeared && scenePhase == .active && !reduceMotion && !transcriptFocused && !confirmDiscard &&
            (capture.phase == .idle || capture.phase == .recording)
    }

    private var recordLabel: String {
        switch capture.phase {
        case .idle: "开始录音"
        case .preparing: "正在准备录音"
        case .recording: "停止录音"
        case .stopped: "录音已停止"
        }
    }

    private var canSave: Bool {
        capture.recordingURL != nil && capture.phase != .preparing && !model.saving && capture.error == nil
    }

    private var transcriptCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("转写文字").font(.headline)
                Spacer()
                Button { transcriptFocused.toggle() } label: {
                    Image(systemName: transcriptFocused ? "checkmark" : "pencil.line")
                        .font(.title3).foregroundStyle(IdeaPalette.secondary)
                        .frame(width: 44, height: 44)
                }
                .accessibilityLabel(transcriptFocused ? "完成编辑" : "编辑转写文字")
                .accessibilityIdentifier("idea-voice-finish-editing")
            }
            ZStack(alignment: .topLeading) {
                if transcript.isEmpty {
                    Text("说出的想法会显示在这里，也可以直接修改。")
                        .foregroundStyle(IdeaPalette.faint).padding(.top, 9)
                        .allowsHitTesting(false)
                }
                TextEditor(text: Binding(get: { transcript }, set: { transcript = $0; userEdited = true }))
                    .font(.body)
                    .scrollContentBackground(.hidden)
                    .frame(height: transcriptEditorHeight)
                    .focused($transcriptFocused)
                    .id("transcript-editor")
                    .accessibilityIdentifier("idea-voice-transcript")
            }
            Text("可以修改后再保存").font(.caption).foregroundStyle(IdeaPalette.secondary)
        }
        .padding(20)
        .background(.white, in: RoundedRectangle(cornerRadius: 26))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("idea-voice-transcript-card")
    }

    private var saveDock: some View {
        VStack(spacing: 9) {
            Button {
                if capture.phase == .recording { capture.stop() }
                guard let url = capture.recordingURL, capture.error == nil else { return }
                Task {
                    if await model.saveVoice(recordingAt: url, duration: capture.duration,
                                             transcript: capture.finalTranscript ?? capture.partial,
                                             transcriptionComplete: capture.finalTranscript != nil,
                                             editedBody: userEdited ? transcript : (capture.finalTranscript ?? capture.partial),
                                             userEdited: userEdited) {
                        capture.finishSaving()
                        model.removeRecovered(url)
                        onSaved()
                    }
                }
            } label: {
                Label(model.saving ? "正在保存…" : "保存到想法库", systemImage: "doc.badge.plus")
                    .font(.headline).frame(maxWidth: .infinity, minHeight: 57)
                    .foregroundStyle(.white)
                    .background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 20))
            }
            .buttonStyle(IdeaPressStyle())
            .disabled(!canSave)
            .animation(.easeOut(duration: 0.18), value: canSave)
            .accessibilityIdentifier("idea-voice-save")
            Text("保存后仅记录想法，不会开始执行")
                .font(.caption).foregroundStyle(IdeaPalette.secondary)
        }
        .padding(.horizontal, 20).padding(.top, 17).padding(.bottom, 7)
        .background(.regularMaterial)
    }

    private func close() {
        if capture.recordingURL == nil {
            capture.discard() // Invalidate permission callbacks before dismissing a pending recording start.
            if transcript.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { dismiss() }
            else { confirmDiscard = true }
        }
        else { confirmDiscard = true }
    }

    private func durationLabel(_ seconds: TimeInterval) -> String {
        let value = max(0, Int(seconds))
        return String(format: "%02d:%02d", value / 60, value % 60)
    }
}
