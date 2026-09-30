import Foundation
import SQLite3
import XCTest
@testable import NormaCore

final class IdeaStoreTests: XCTestCase {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func store(in directory: URL) -> IdeaStore {
        IdeaStore(databaseURL: directory.appendingPathComponent("ideas.sqlite"),
                  recordingsDirectory: directory.appendingPathComponent("IdeaRecordings", isDirectory: true))
    }

    /// 离线文字想法可编辑、搜索并在重新打开数据库后保持独立记录。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   2 条想法 = 1 条含「首屏」的想法 + 1 条含「连接」的想法；搜索「首屏」应只命中 1 条。
    ///   两条记录使用同一临时数据库，未提供任何 Mac、项目、Workspace 或任务身份。
    ///
    /// 执行过程：
    ///   1. 保存两条文字想法 → 数据库出现两条独立记录。
    ///   2. 只编辑第一条 → 搜索新词并重新创建 Store 读取。
    ///   3. 尝试保存空白文字 → 应拒绝且不影响原记录。
    ///
    /// 预期结果：第一条的新正文和稳定 ID 保留；第二条原文不变；不匹配的搜索词无结果；空白保存失败。
    func testTextIdeasSurviveEditSearchAndRestartWithoutWorkspace() async throws {
        let directory = try directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let firstTime = Date(timeIntervalSince1970: 1_000)
        let secondTime = Date(timeIntervalSince1970: 2_000)
        let ideas = store(in: directory)
        let first = try await ideas.createText("首屏信息太多", at: firstTime)
        let second = try await ideas.createText("第一次连接 Mac 要说明安全性", at: secondTime)
        let edited = try await ideas.updateBody(id: first.id, body: "首屏交接要更清楚", at: Date(timeIntervalSince1970: 3_000))
        let reopened = store(in: directory)
        let all = try await reopened.all()
        let matches = try await reopened.search("首屏")

        XCTAssertEqual(all.count, 2, "重启后两条独立想法都必须存在")
        XCTAssertEqual(all.first?.id, first.id, "按修改时间倒序时编辑过的想法应排第一")
        XCTAssertEqual(edited.id, first.id, "编辑不能把想法替换成新的身份")
        XCTAssertEqual(all.first?.body, "首屏交接要更清楚", "修改后的正文必须从磁盘恢复")
        XCTAssertEqual(all.last?.body, second.body, "编辑第一条不能改变第二条")
        XCTAssertEqual(matches.map(\.id), [first.id], "搜索只能命中包含首屏的想法")
        let unrelated = try await reopened.search("不存在的词")
        XCTAssertTrue(unrelated.isEmpty, "不匹配的查询不能返回其他想法")
        do {
            _ = try await reopened.createText("  \n  ")
            XCTFail("空白正文不应被保存")
        } catch {
            let remaining = try await reopened.all()
            XCTAssertEqual(remaining.count, 2, "空白保存失败不能破坏已有想法")
        }
    }

    /// 语音识别失败后原始录音和待转写状态跨重启保留，后续转写不覆盖用户编辑。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   4 字节测试录音用于证明文件字节被保留；时长 1.5 秒为正值；1 条语音想法无初始转写。
    ///   用户先手工输入「保留我的修改」，晚到的识别结果为「机器转写」。
    ///
    /// 执行过程：
    ///   1. 保存录音且无转写 → 标为待转写。
    ///   2. 标记识别失败并编辑正文 → 重启 Store 检查音频与状态。
    ///   3. 重试识别成功 → 原始识别结果保存，用户正文不被覆盖。
    ///
    /// 预期结果：录音字节、语音类型、失败状态和用户正文保留；文字想法搜索不混入无关词。
    func testVoiceRecordingSurvivesFailureRestartAndLateTranscript() async throws {
        let directory = try directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let source = directory.appendingPathComponent("capture.caf")
        let audio = Data([1, 2, 3, 4])
        try audio.write(to: source)
        let ideas = store(in: directory)
        let voice = try await ideas.createVoice(recordingAt: source, duration: 1.5, transcript: nil)
        XCTAssertEqual(voice.kind, .voice, "有录音的想法必须标为语音")
        XCTAssertEqual(voice.transcriptionState, .pending, "无识别结果时必须可保存为待转写")
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path), "保存成功前的原录音仍应保留供调用方确认后清理")
        _ = try await ideas.markTranscriptionFailed(id: voice.id)
        _ = try await ideas.updateBody(id: voice.id, body: "保留我的修改")

        let reopened = store(in: directory)
        let saved = try await reopened.all()
        let restored = try XCTUnwrap(saved.first)
        let savedRecording = try await reopened.recordingURL(for: restored.id)
        let recording = try XCTUnwrap(savedRecording)
        XCTAssertEqual(restored.id, voice.id, "重启后应恢复同一条语音想法")
        XCTAssertEqual(restored.transcriptionState, .failed, "识别失败状态必须跨重启保留")
        XCTAssertEqual(restored.body, "保留我的修改", "用户手工编辑的正文必须保留")
        XCTAssertEqual(try Data(contentsOf: recording), audio, "原始录音字节不能因识别失败而丢失")

        let recognized = try await reopened.updateTranscript(id: voice.id, transcript: "机器转写")
        XCTAssertEqual(recognized.rawTranscript, "机器转写", "重试识别结果必须单独保留")
        XCTAssertEqual(recognized.body, "保留我的修改", "晚到转写不能覆盖用户修改")
        XCTAssertEqual(recognized.transcriptionState, .ready, "重试成功后状态应变为已完成")
        let unrelated = try await reopened.search("无关词")
        XCTAssertTrue(unrelated.isEmpty, "无关搜索词不能误命中语音想法")
    }

    /// 用户正在修改实时转写时仍可保存录音，待转写状态与已编辑正文须一次落盘。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   4 字节录音与 1 条用户正文「我改过的句子」；最终转写尚未到达，因此原始转写为空。
    ///
    /// 执行过程：保存语音录音和已编辑正文 → 重建 Store → 查询同一条记录。
    ///
    /// 预期结果：用户正文存在，状态仍为待转写，原始转写不伪造为用户文字，音频仍在。
    func testEditedPartialVoiceCanBeSavedWhileRecognitionIsPending() async throws {
        let directory = try directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let source = directory.appendingPathComponent("capture.caf")
        try Data([1, 2, 3, 4]).write(to: source)
        let idea = try await store(in: directory).createVoice(
            recordingAt: source, duration: 2, transcript: nil, editedBody: "我改过的句子", userEdited: true)
        let reopened = store(in: directory)
        let records = try await reopened.all()
        XCTAssertEqual(records.first?.id, idea.id, "待转写语音应以稳定身份恢复")
        XCTAssertEqual(records.first?.body, "我改过的句子", "保存时用户修改必须随录音一次落盘")
        XCTAssertEqual(records.first?.transcriptionState, .pending, "尚无最终识别结果时不得假报已完成")
        XCTAssertNil(records.first?.rawTranscript, "用户输入不能冒充 Apple 原始转写")
        let audio = try await reopened.recordingURL(for: idea.id)
        XCTAssertNotNil(audio, "待转写记录必须仍指向原始录音")
    }

    /// 待转写的部分识别文字在重试成功后补全，用户改写过的正文则原样保留。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1 份 4 字节测试录音生成 2 条独立语音想法；两条均先得到「先做」部分结果。
    ///   第一条未经编辑，第二条由用户改为「先修 API」；重试结果均为「先做 iOS 首屏」。
    ///
    /// 执行过程：
    ///   1. 保存两条待转写录音，分别记录用户是否改写正文。
    ///   2. 对两条想法写入完整识别结果，再重建 Store 读取。
    ///
    /// 预期结果：未编辑正文更新为完整转写；用户改写正文不被覆盖；两条原始转写与状态均更新。
    func testPartialRecognitionOnlyReplacesUneditedBody() async throws {
        let directory = try directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let source = directory.appendingPathComponent("capture.caf")
        try Data([1, 2, 3, 4]).write(to: source)
        let ideas = store(in: directory)
        let untouched = try await ideas.createVoice(recordingAt: source, duration: 1.5,
            transcript: "先做", transcriptionComplete: false, editedBody: "先做", userEdited: false)
        let edited = try await ideas.createVoice(recordingAt: source, duration: 1.5,
            transcript: "先做", transcriptionComplete: false, editedBody: "先修 API", userEdited: true)
        XCTAssertEqual(untouched.transcriptionState, .pending, "部分识别结果不能冒充已完成转写")
        XCTAssertEqual(edited.transcriptionState, .pending, "用户改写正文不代表识别已完成")

        _ = try await ideas.updateTranscript(id: untouched.id, transcript: "先做 iOS 首屏")
        _ = try await ideas.updateTranscript(id: edited.id, transcript: "先做 iOS 首屏")
        let restored = try await store(in: directory).all()
        let restoredUntouched = try XCTUnwrap(restored.first { $0.id == untouched.id })
        let restoredEdited = try XCTUnwrap(restored.first { $0.id == edited.id })
        XCTAssertEqual(restoredUntouched.body, "先做 iOS 首屏", "未编辑正文应使用完整转写补全")
        XCTAssertEqual(restoredEdited.body, "先修 API", "重新转写不得覆盖用户明确修改的正文")
        XCTAssertEqual(restoredUntouched.rawTranscript, "先做 iOS 首屏", "完整识别原文必须单独保存")
        XCTAssertEqual(restoredEdited.rawTranscript, "先做 iOS 首屏", "用户改写时仍须保存新的原始识别结果")
        XCTAssertEqual(restoredUntouched.transcriptionState, .ready, "未编辑想法的识别状态应变为完成")
        XCTAssertEqual(restoredEdited.transcriptionState, .ready, "已编辑想法的识别状态也应变为完成")
    }

    /// 旧版数据库升级后保留原有语音及用户修改，不能把旧数据全部误判为未编辑。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   旧表有 2 条语音：1 条正文等于原始部分转写，1 条正文由用户改写；旧表没有编辑标志。
    ///   两条录音时长均为 1 秒，创建与修改时间均为 Unix 时间 1000 秒。
    ///
    /// 执行过程：
    ///   1. 直接写入旧版 SQLite 表和两条记录。
    ///   2. 用新 Store 打开触发迁移，再为两条记录写入完整转写。
    ///
    /// 预期结果：两条记录仍存在；未编辑正文补全；用户改写正文仍在；旧音频引用不变。
    func testLegacyVoiceMigrationPreservesEditedBody() async throws {
        let directory = try directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = directory.appendingPathComponent("ideas.sqlite")
        let plainID = "00000000-0000-0000-0000-000000000001"
        let editedID = "00000000-0000-0000-0000-000000000002"
        var connection: OpaquePointer?
        XCTAssertEqual(sqlite3_open(database.path, &connection), SQLITE_OK, "测试必须能创建旧数据库")
        let db = try XCTUnwrap(connection)
        let legacySQL = """
            CREATE TABLE ideas (id TEXT PRIMARY KEY NOT NULL, kind TEXT NOT NULL, body TEXT NOT NULL,
              raw_transcript TEXT, transcription_state TEXT NOT NULL, audio_filename TEXT,
              duration REAL, created_at REAL NOT NULL, updated_at REAL NOT NULL);
            INSERT INTO ideas VALUES ('\(plainID)', 'voice', '旧部分', '旧部分', 'pending', '\(plainID).caf', 1, 1000, 1000);
            INSERT INTO ideas VALUES ('\(editedID)', 'voice', '用户改写', '旧部分', 'pending', '\(editedID).caf', 1, 1000, 1000);
            """
        XCTAssertEqual(sqlite3_exec(db, legacySQL, nil, nil, nil), SQLITE_OK, "旧版数据夹具必须写入成功")
        XCTAssertEqual(sqlite3_close(db), SQLITE_OK, "迁移前必须关闭夹具连接")

        let ideas = store(in: directory)
        let migrated = try await ideas.all()
        XCTAssertEqual(migrated.count, 2, "迁移不能丢失任何旧语音想法")
        _ = try await ideas.updateTranscript(id: plainID, transcript: "完整的机器转写")
        _ = try await ideas.updateTranscript(id: editedID, transcript: "完整的机器转写")
        let restored = try await store(in: directory).all()
        let plain = try XCTUnwrap(restored.first { $0.id == plainID })
        let edited = try XCTUnwrap(restored.first { $0.id == editedID })
        XCTAssertEqual(plain.body, "完整的机器转写", "原文未改的旧语音应允许自动补全")
        XCTAssertEqual(edited.body, "用户改写", "旧版用户正文不能被重新转写覆盖")
        XCTAssertEqual(plain.audioFilename, "\(plainID).caf", "迁移不能改动第一条录音引用")
        XCTAssertEqual(edited.audioFilename, "\(editedID).caf", "迁移不能改动第二条录音引用")
    }

    /// 损坏的想法数据库及失败录音源不能被视为空库，也不能删除用户源文件。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   8 字节无效数据库正文「damaged!」；不存在的录音路径和 4 字节仍在本机的源录音作为失败输入。
    ///
    /// 执行过程：
    ///   1. 对不存在的录音尝试保存 → 必须失败且没有语音记录。
    ///   2. 写入无效数据库字节 → 尝试保存文字与已有录音。
    ///   3. 重新读取磁盘 → 无效原文件与源录音仍保持原样。
    ///
    /// 预期结果：不能产生无录音的语音想法；不能用新空库覆盖损坏的旧数据，失败时不能删除源录音。
    func testMissingAudioAndDamagedDatabaseDoNotCreateFalseIdeas() async throws {
        let directory = try directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let ideas = store(in: directory)
        let missing = directory.appendingPathComponent("missing.caf")
        do {
            _ = try await ideas.createVoice(recordingAt: missing, duration: 1, transcript: nil)
            XCTFail("不存在的录音不能生成语音想法")
        } catch {
            let remaining = try await ideas.all()
            XCTAssertTrue(remaining.isEmpty, "录音保存失败不能留下空白语音记录")
        }
        let database = directory.appendingPathComponent("ideas.sqlite")
        let damaged = Data("damaged!".utf8)
        try damaged.write(to: database)
        do {
            _ = try await store(in: directory).createText("不得覆盖")
            XCTFail("损坏数据库不能被当作空想法库")
        } catch {
            XCTAssertEqual(try Data(contentsOf: database), damaged, "损坏的原始数据库字节必须保留")
        }
        let source = directory.appendingPathComponent("recorded.caf")
        let audio = Data([4, 3, 2, 1])
        try audio.write(to: source)
        do {
            _ = try await store(in: directory).createVoice(recordingAt: source, duration: 1, transcript: nil)
            XCTFail("数据库损坏时不能假报录音已保存为想法")
        } catch {
            XCTAssertEqual(try Data(contentsOf: source), audio, "数据库写入失败也必须保留原始录音供恢复")
            let copies = try FileManager.default.contentsOfDirectory(atPath: directory.appendingPathComponent("IdeaRecordings").path)
            XCTAssertTrue(copies.isEmpty, "未提交的录音副本不能伪装为已保存的想法")
            XCTAssertEqual(try Data(contentsOf: database), damaged, "录音失败同样不能覆写损坏数据库")
        }
    }

    #if os(iOS)
    /// 物理 iPhone 上的想法数据库与语音原件均只能在解锁时读取。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1 条短文字 + 1 段 4 字节测试录音；两个目标文件分别检查完整保护等级。
    ///
    /// 执行过程：真机保存文字与语音 → 读取文件保护属性；模拟器明确跳过。
    ///
    /// 预期结果：数据库与录音均为 NSFileProtectionComplete，不能降为首次解锁后可读。
    func testIdeaFilesUseCompleteProtectionOnPhysicalIPhone() async throws {
        #if targetEnvironment(simulator)
        throw XCTSkip("模拟器文件保护属性不代表物理 iPhone；本项须真机验收")
        #else
        let directory = try directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let ideas = store(in: directory)
        _ = try await ideas.createText("仅保存在我的手机")
        let source = directory.appendingPathComponent("source.caf")
        try Data([1, 2, 3, 4]).write(to: source)
        let voice = try await ideas.createVoice(recordingAt: source, duration: 1, transcript: nil)
        let savedRecording = try await ideas.recordingURL(for: voice.id)
        let recording = try XCTUnwrap(savedRecording)
        let database = directory.appendingPathComponent("ideas.sqlite")
        let dbProtection = try FileManager.default.attributesOfItem(atPath: database.path)[.protectionKey] as? FileProtectionType
        let audioProtection = try FileManager.default.attributesOfItem(atPath: recording.path)[.protectionKey] as? FileProtectionType
        XCTAssertEqual(dbProtection, .complete, "想法数据库必须采用完整文件保护")
        XCTAssertEqual(audioProtection, .complete, "原始录音不能采用较弱保护级别")
        XCTAssertNotEqual(dbProtection, .completeUntilFirstUserAuthentication, "解锁一次后持续可读不满足本地隐私要求")
        #endif
    }
    #endif
}
