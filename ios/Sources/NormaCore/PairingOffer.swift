import Foundation

public enum MobileError: Error, LocalizedError, Sendable, Equatable {
    case terminalChanged, terminalUnavailable, terminalControlExpired, terminalControlled, desktopInputPending
    case pushSubscriptionChanged, invalidPushRegistration
    case invalidComputerAddress, invalidPairingCode
    case invalidOffer, unsupportedVersion, invalidResponse, wrongComputer, unauthorized
    case pairingUnavailable, verificationMismatch, connectionFailed, timedOut
    case keychain(Int32), invalidCredential
    case unavailable, snapshotTooLarge
    case grantChanged, commandConflict, commandNotFound, busy
    case historyUnavailable, historyTooLarge, sessionChanged
    case reviewTooLarge, reviewUnavailable, reviewChanged, reviewUnsupported
    case approvalChanged, approvalUnavailable, approvalTooLarge, decisionUnavailable
    case queuePending, queueNotCancellable, queueUnavailable
    case todoDecided, invalidTodo
    case eventGap
    public var errorDescription: String? {
        switch self {
        case .terminalControlExpired: "终端接管已结束，请重新接管后输入。"
        case .terminalControlled: "终端正由其他连接接管，请交还后重试。"
        case .desktopInputPending: "电脑终端有尚未处理的输入，请先在电脑上处理。"
        case .terminalChanged: "终端已变化，请查看最新屏幕后重新输入。"
        case .terminalUnavailable: "终端暂不可用，请确认电脑使用最新版本，且原卡已启动。"
        case .pushSubscriptionChanged: "提醒设置已变化，请重新读取后确认。"
        case .invalidPushRegistration: "通知设备信息无效，请重新开启系统通知后重试。"
        case .invalidComputerAddress: "请输入电脑上显示的局域网或 Tailscale 地址。"
        case .invalidPairingCode: "验证码不正确或已失效。每组码最多尝试 3 次，请在电脑上刷新验证码。"
        case .todoDecided: "这项建议已经作出决定，请查看最新进度。"
        case .invalidTodo: "计划或目标已变化，请刷新后重新确认。"
        case .eventGap: "项目同步出现缺口，正在重新读取完整状态。"
        case .approvalChanged: "这项请求已处理、过期或会话已变化，请刷新。"
        case .approvalUnavailable: "暂时无法读取审批，请稍后刷新。"
        case .approvalTooLarge: "审批内容超过当前读取上限，请在电脑查看完整内容。"
        case .decisionUnavailable: "Agent 没有提供这个选项，请刷新请求。"
        case .queuePending: "这张卡片还有排队消息，请查询后加入队列。"
        case .queueNotCancellable: "消息已经开始发送，无法安全撤回。请查询最新结果。"
        case .queueUnavailable: "电脑暂时无法保存队列，可能已达容量上限。原文保留在手机，请先查询结果。"
        case .historyUnavailable: "对话暂时无法读取，请刷新后重试。"
        case .reviewTooLarge: "变更超出预览上限，请在电脑上核对。内容没有被截断。"
        case .reviewUnavailable: "暂时无法读取变更，请确认项目是 Git 仓库后重试。"
        case .reviewChanged: "读取期间项目发生变化，请重新预览。"
        case .reviewUnsupported: "此项目包含暂不支持的内容、子模块或 Git 过滤器，请在电脑上核对。"
        case .historyTooLarge: "这页对话超出传输上限，请先在电脑上查看。内容没有被截断。"
        case .sessionChanged: "会话或项目窗口已变化，请重新打开卡片。"
        case .invalidOffer: "这不是有效的 Norma 配对信息。请在电脑上重新生成。"
        case .unsupportedVersion: "电脑与手机的版本不兼容，请更新 Norma。"
        case .invalidResponse: "电脑返回了无法识别的响应。"
        case .wrongComputer: "电脑身份与已保存的身份不一致，连接已停止。"
        case .unauthorized: "此手机尚未获得授权，或授权已被撤销。"
        case .pairingUnavailable: "配对信息已失效或已被使用。请在电脑上重新生成。"
        case .verificationMismatch: "核对码不一致，配对已停止。"
        case .connectionFailed: "无法连接这台电脑。请确认 Norma 已打开，且局域网或 Tailscale 可用。"
        case .timedOut: "等待电脑响应超时。已发送的操作请查询结果。"
        case .keychain: "无法读取或保存设备凭据，请解锁手机后重试。"
        case .invalidCredential: "设备凭据无效，请忘记此电脑后重新配对。"
        case .unavailable: "电脑暂时无法处理请求。已发送的操作请查询结果。"
        case .snapshotTooLarge: "这个项目的卡片数据超过当前读取上限，请先在电脑上查看。"
        case .grantChanged: "电脑上的项目权限已改变，请刷新项目后再操作。"
        case .commandConflict: "这个操作编号已用于其他内容，请先核对原操作结果。"
        case .commandNotFound: "电脑尚无此操作的回执。请先在电脑上核对，手机不会自动重新发送。"
        case .busy: "电脑正在处理其他操作，请稍后查询结果。"
        }
    }
}

public struct PairingOffer: Decodable, Sendable, CustomStringConvertible {
    public let machineID: String
    public let machineName: String
    public let certificateSHA256: String
    public let endpoints: [URL]
    let offerID: String
    let secret: String
    let expiresInSeconds: Int
    private let version: Int
    enum CodingKeys: String, CodingKey, CaseIterable {
        case machineID = "machine_id", machineName = "machine_name", certificateSHA256 = "certificate_sha256"
        case offerID = "offer_id", secret, endpoints, version, expiresInSeconds = "expires_in_seconds"
    }
    public var description: String { "PairingOffer(machine: \(machineName))" }
    public static func decode(_ data: Data) throws -> Self {
        // Four endpoints of at most 256 bytes plus names/keys fit below 4 KiB.
        guard data.count <= 4096,
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set(CodingKeys.allCases.map(\.rawValue)) else { throw MobileError.invalidOffer }
        return try JSONDecoder().decode(Self.self, from: data)
    }
    public init(from decoder: any Decoder) throws {
        let fields = try decoder.container(keyedBy: CodingKeys.self)
        version = try fields.decode(Int.self, forKey: .version)
        machineID = try fields.decode(String.self, forKey: .machineID)
        machineName = try fields.decode(String.self, forKey: .machineName)
        certificateSHA256 = try fields.decode(String.self, forKey: .certificateSHA256)
        endpoints = try fields.decode([URL].self, forKey: .endpoints)
        offerID = try fields.decode(String.self, forKey: .offerID)
        secret = try fields.decode(String.self, forKey: .secret)
        expiresInSeconds = try fields.decode(Int.self, forKey: .expiresInSeconds)
        guard version == 1 else { throw MobileError.unsupportedVersion }
        guard validHex(certificateSHA256), machineID == "mac-" + certificateSHA256,
              validHex(offerID), validHex(secret), validName(machineName),
              (1...120).contains(expiresInSeconds), !endpoints.isEmpty, endpoints.count <= 4,
              endpoints.allSatisfy(validEndpoint) else { throw MobileError.invalidOffer }
    }
}

func validHex(_ value: String) -> Bool {
    value.utf8.count == 64 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
}
func validName(_ value: String) -> Bool {
    !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && value.count <= 80 &&
    !value.unicodeScalars.contains { CharacterSet.controlCharacters.contains($0) }
}
func validEndpoint(_ url: URL) -> Bool {
    url.absoluteString.utf8.count <= 256 && url.scheme == "wss" && url.host != nil &&
    url.user == nil && url.password == nil && url.path == "/mobile" && url.query == nil &&
    url.fragment == nil && url.port != 0
}
