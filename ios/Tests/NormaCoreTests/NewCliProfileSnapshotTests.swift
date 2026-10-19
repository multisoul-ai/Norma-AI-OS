import XCTest
@testable import NormaCore

final class NewCliProfileSnapshotTests: XCTestCase {
    private let newProfiles = ["Kimi", "ZCode", "Pi", "Hermes"]

    private func card(_ profile: String, node: Int = 7) -> String {
        #"{"node_id":\#(node),"profile":"\#(profile)","title":"卡片","mobile_control":false,"state":"idle"}"#
    }

    /// Mac 上出现 Kimi/ZCode/Pi/Hermes 卡片时，单张卡片校验必须通过，不能因白名单缺项而抛错。
    ///
    /// 数据构造：node_id=7、state=idle 的最小卡片 JSON，profile 依次取四个新值；
    /// 另构造 profile=Nope 的卡片作为负例。
    /// 执行过程：JSON 解码为 CardSnapshot → 调用 validate()。
    /// 预期结果：四个新 profile 都通过；未知 profile 仍被拒绝（白名单没有被放开成任意字符串）。
    func testCardValidationAcceptsNewCliProfilesButStillRejectsUnknown() throws {
        for profile in newProfiles {
            let decoded = try JSONDecoder().decode(CardSnapshot.self, from: Data(card(profile).utf8))
            XCTAssertNoThrow(try decoded.validate(), "\(profile) 卡片必须能通过校验，否则整份手机快照会失败")
        }
        let unknown = try JSONDecoder().decode(CardSnapshot.self, from: Data(card("Nope").utf8))
        XCTAssertThrowsError(try unknown.validate(), "未知 profile 仍应被拒绝")
    }

    /// 项目快照同时包含已支持卡片和新 CLI 卡片时，整份快照必须仍然有效。
    ///
    /// 数据构造：card_count=2；一张 Codex（node 7）、一张 Kimi（node 8）；project-a / route-a。
    /// 执行过程：解码 ProjectSnapshot → validate(includeCards: true)。
    /// 预期结果：校验通过，两张卡片都保留；把其中一张换成未知 profile 后整份快照被拒绝（负断言）。
    func testProjectSnapshotWithNewCliCardStaysValid() throws {
        func project(_ second: String) -> String {
            #"{"workspace_id":"project-a","display_name":"项目","state":"open","route_id":"route-a","card_count":2,"cards":[\#(card("Codex", node: 7)),\#(card(second, node: 8))]}"#
        }
        for profile in newProfiles {
            let snapshot = try JSONDecoder().decode(ProjectSnapshot.self, from: Data(project(profile).utf8))
            XCTAssertNoThrow(try snapshot.validate(includeCards: true), "含 \(profile) 卡片的项目快照必须有效")
            XCTAssertEqual(snapshot.cards?.count, 2, "两张卡片都应保留，不能因新卡片被丢弃")
        }
        let bad = try JSONDecoder().decode(ProjectSnapshot.self, from: Data(project("Nope").utf8))
        XCTAssertThrowsError(try bad.validate(includeCards: true), "含未知 profile 的快照仍应被拒绝")
    }

    /// agent 定义列表含新 CLI 时校验通过，但手机端仍不能创建这些卡片。
    ///
    /// 数据构造：definition_id=3 的启用定义，profile 取四个新值，以及一个 Codex 定义作对照。
    /// 执行过程：解码 ProjectAgentDefinition → validate() → 读取 isCreatableOnIOS。
    /// 预期结果：新 profile 校验通过且 isCreatableOnIOS=false；Codex 为 true；
    /// MobileAgentProfile.supports 对四个新值返回 false，证明没有顺带扩大手机端能力。
    func testAgentDefinitionAcceptsNewCliProfilesWithoutEnablingIOSCreation() throws {
        func definition(_ profile: String) -> String {
            #"{"definition_id":3,"name":"代理","profile":"\#(profile)","enabled":true,"busy":false}"#
        }
        for profile in newProfiles {
            let decoded = try JSONDecoder().decode(ProjectAgentDefinition.self, from: Data(definition(profile).utf8))
            XCTAssertNoThrow(try decoded.validate(), "\(profile) 定义必须能通过校验")
            XCTAssertFalse(decoded.isCreatableOnIOS, "\(profile) 不应出现在手机端可创建列表")
            XCTAssertFalse(MobileAgentProfile.supports(profile), "\(profile) 的手机端能力未验证，不得放开")
        }
        let codex = try JSONDecoder().decode(ProjectAgentDefinition.self, from: Data(definition("Codex").utf8))
        XCTAssertTrue(codex.isCreatableOnIOS, "对照：Codex 仍可在手机端创建")
        let unknown = try JSONDecoder().decode(ProjectAgentDefinition.self, from: Data(definition("Nope").utf8))
        XCTAssertThrowsError(try unknown.validate(), "未知 profile 的定义仍应被拒绝")
    }
}
