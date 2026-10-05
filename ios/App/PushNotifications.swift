import SwiftUI
import UIKit
import UserNotifications
import NormaCore

@MainActor final class PushNotifications: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate, ObservableObject {
    let computers = ComputerModel()
    let activityWorkspaces = WorkspaceChatModel()
    let activityCloud = CloudChatModel()
    let activities = WorkspaceActivityManager()
    @Published var authorization: UNAuthorizationStatus = .notDetermined
    @Published var registration: APNsRegistration?
    @Published var registering = false
    @Published var systemError: String?
    @Published var target: PushNotificationTarget?
    @Published var subscriptions: [String: PushSubscriptionState] = [:]
    @Published var errors: [String: String] = [:]
    @Published var busy: Set<String> = []
    private var enabled = Set(UserDefaults.standard.stringArray(forKey: "notification-machines") ?? [])
    private var renewAfterOperation: Set<String> = []
    private var pendingRemoval: [String: @Sendable () async throws -> Void] = [:]
    var tokenChanged: (() -> Void)?
    var systemAllowed: Bool { [.authorized, .provisional, .ephemeral].contains(authorization) }

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        // ActivityKit can wake the process without a SwiftUI scene. Install token observers here.
        activities.attach(workspaces:activityWorkspaces,computers:computers,cloud:activityCloud)
        return true
    }
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        registering = false
        guard let environment = environment() else { return }
        let value = APNsRegistration(token: deviceToken.map { String(format: "%02x", $0) }.joined(), environment: environment)
        do { try value.validate(); registration = value; systemError = nil; tokenChanged?() }
        catch { registration = nil; systemError = "系统返回了无效的通知登记结果。" }
    }
    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: any Error) {
        registering = false; registration = nil
        systemError = "无法登记系统通知：" + error.localizedDescription
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification,
                                           withCompletionHandler completionHandler: @escaping @Sendable (UNNotificationPresentationOptions) -> Void) {
        Task { @MainActor in completionHandler([.banner, .sound]) }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
                                           withCompletionHandler completionHandler: @escaping @Sendable () -> Void) {
        let object = response.notification.request.content.userInfo["norma"]
        let data = object.flatMap { try? JSONSerialization.data(withJSONObject: $0) }
        let value = response.actionIdentifier == UNNotificationDefaultActionIdentifier
            ? data.flatMap { try? PushNotificationTarget.decode($0) } : nil
        Task { @MainActor in
            if let value { self.target = value }
            // UIKit finishes cold-launch state restoration inside this callback. Completing an
            // async delegate on its generic executor crashes with "Call must be made on main thread".
            completionHandler()
        }
    }
    func refreshSystem() async {
        authorization = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
        if systemAllowed && !enabled.isEmpty && registration == nil { register() }
        if !systemAllowed { registration = nil }
    }
    func requestPermission() async {
        systemError = nil
        do {
            let accepted = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])
            await refreshSystem()
            if accepted { register() }
        } catch { systemError = error.localizedDescription }
    }
    private func environment() -> APNsEnvironment? {
        let name = Bundle.main.object(forInfoDictionaryKey: "NormaAPNsEnvironment") as? String
        guard let environment = name == "development" ? APNsEnvironment.sandbox : name == "production" ? .production : nil else {
            systemError = "此构建尚未配置推送环境。"; return nil
        }
        return environment
    }
    func register() {
        guard systemAllowed, !registering, environment() != nil else { return }
        registering = true
        UIApplication.shared.registerForRemoteNotifications()
    }
    func load(_ machines: [PairedMachine], client: MobileClient) async {
        for machine in machines where !busy.contains(machine.id) {
            busy.insert(machine.id)
            do {
                let state = try await client.pushSubscription(machine)
                try Task.checkCancellation()
                subscriptions[machine.id] = state; errors[machine.id] = nil
            } catch { if !Task.isCancelled { subscriptions[machine.id] = nil; errors[machine.id] = error.localizedDescription } }
            finish(machine.id)
            if Task.isCancelled { return }
        }
    }
    func synchronize(_ machines: [PairedMachine], client: MobileClient) async {
        guard let registration, systemAllowed else { return }
        for machine in machines where enabled.contains(machine.id) {
            if busy.contains(machine.id) { renewAfterOperation.insert(machine.id); continue }
            _ = await update(machine, to: registration, client: client, renewal: true)
        }
    }
    /// User intent is recorded before transmission; only an already-enabled Mac may receive token renewal.
    @discardableResult func update(_ machine: PairedMachine, to value: APNsRegistration?, client: MobileClient, renewal: Bool = false) async -> Bool {
        guard !busy.contains(machine.id), pendingRemoval[machine.id] == nil else { return false }
        busy.insert(machine.id); errors[machine.id] = nil
        defer { finish(machine.id) }
        if !renewal { setEnabled(machine.id, value != nil) }
        do {
            let current = try await client.pushSubscription(machine)
            try Task.checkCancellation()
            if renewal {
                guard enabled.contains(machine.id) else { return false }
                guard current.registration != nil else {
                    setEnabled(machine.id, false); subscriptions[machine.id] = current; return true
                }
            }
            if current.registration == value { subscriptions[machine.id] = current }
            else { subscriptions[machine.id] = try await client.updatePushSubscription(machine, expected: current, registration: value) }
            if value == nil { await clearDelivered(machine.id) }
            return true
        } catch {
            subscriptions[machine.id] = nil
            if value == nil && !renewal && (error as? MobileError) == .unauthorized {
                errors[machine.id] = "此手机已无可用授权，可忘记此电脑。"
                await clearDelivered(machine.id)
                return true
            }
            errors[machine.id] = error.localizedDescription + " 请重新核验电脑上的提醒设置。"
            return false
        }
    }
    private func setEnabled(_ machine: String, _ value: Bool) {
        if value { enabled.insert(machine) } else { enabled.remove(machine) }
        UserDefaults.standard.set(Array(enabled), forKey: "notification-machines")
    }
    func forget(_ machine: String, cleanup: (@Sendable () async throws -> Void)?) {
        setEnabled(machine, false)
        renewAfterOperation.remove(machine)
        subscriptions[machine] = nil; errors[machine] = nil
        if target?.machineID == machine { target = nil }
        pendingRemoval[machine] = cleanup ?? {}
        Task { await clearDelivered(machine) }
        if !busy.contains(machine) { finish(machine) }
    }
    private func finish(_ machine: String) {
        busy.remove(machine)
        if let cleanup = pendingRemoval.removeValue(forKey: machine) {
            subscriptions[machine] = nil; errors[machine] = nil
            // Let an existing registration finish first, so it cannot re-enable after cleanup.
            Task {
                do { try await cleanup() }
                catch { /* ponytail: one attempt; offline cleanup stays on Mac, without retaining credentials for retries. */ }
            }
        } else if renewAfterOperation.remove(machine) != nil { tokenChanged?() }
    }
    func clearDelivered(_ machine: String) async {
        let center = UNUserNotificationCenter.current()
        let delivered = await center.deliveredNotifications()
        let identifiers = delivered.filter {
            ($0.request.content.userInfo["norma"] as? [String: Any])?["machine_id"] as? String == machine
        }.map { $0.request.identifier }
        center.removeDeliveredNotifications(withIdentifiers: identifiers)
    }
}
