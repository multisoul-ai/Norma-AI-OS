import XCTest
import UIKit
@testable import Norma

final class NotificationSystemTests: XCTestCase {
    /// 原生APNs成功回调才生成登记；失败后清除旧token，不声称手机已经订阅。
    ///
    /// 数据构造：32字节假token=64位hex；此测试目标Debug环境为development/sandbox。
    /// 执行过程：创建独立delegate → 注入成功回调 → 注入失败回调；不请求系统权限或连接Apple。
    /// 预期结果：成功时发起一次同步信号但无电脑订阅；失败后无可用token，错误明确可见。
    @MainActor func testSystemTokenCallbacksDoNotInventSubscriptions() {
        let notifications = PushNotifications()
        var signals = 0
        notifications.tokenChanged = { signals += 1 }
        notifications.application(UIApplication.shared, didRegisterForRemoteNotificationsWithDeviceToken: Data(repeating: 0xab, count: 32))
        XCTAssertEqual(notifications.registration?.token, String(repeating: "ab", count: 32), "应完整编码系统token")
        XCTAssertEqual(notifications.registration?.environment, .sandbox, "Debug entitlement必须映射到sandbox")
        XCTAssertEqual(signals, 1, "成功回调只通知一次同步")
        XCTAssertTrue(notifications.subscriptions.isEmpty, "系统登记不等于用户已订阅任何电脑")
        XCTAssertNil(notifications.systemError, "合法回调不应报错")
        notifications.application(UIApplication.shared, didFailToRegisterForRemoteNotificationsWithError: NSError(domain: "test", code: 1))
        XCTAssertNil(notifications.registration, "失败后不能拿旧token声称可以注册")
        XCTAssertNotNil(notifications.systemError, "系统登记失败必须可见")
        XCTAssertFalse(notifications.registering, "失败必须退出等待状态")
    }
}
