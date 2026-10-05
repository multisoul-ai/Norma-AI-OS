import SwiftUI
import ActivityKit
import NormaCore

@MainActor @Observable final class WorkspaceActivityManager {
    private(set) var machineID: String?
    private(set) var registration: WorkspaceActivityRegistration?
    private(set) var enabled = ActivityAuthorizationInfo().areActivitiesEnabled
    private(set) var busy = false
    var error: String?
    private var startToken: String?
    private var send: ((String, WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse)?
    private var observers: [String: Task<Void, Never>] = [:]
    private var refreshNeeded = false
    private var lastRead = Date.distantPast
    private let defaults: UserDefaults
    private let settingsKey = "workspace-live-activity"
    private struct Settings: Codable { let machine: String; let registration: WorkspaceActivityRegistration }
    init(defaults: UserDefaults = .standard,send:((String,WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse)? = nil) {
        self.defaults=defaults
        self.send=send
        if let data=defaults.data(forKey:settingsKey),let saved=try? JSONDecoder().decode(Settings.self,from:data) {machineID=saved.machine;registration=saved.registration}
    }
    func attach(workspaces: WorkspaceChatModel, computers: ComputerModel, cloud: CloudChatModel) {
        guard send==nil else {return}
        send={machine,request in
            if !computers.loaded {await computers.load()}
            return try await workspaces.activity(machineID:machine,request:request,computers:computers,cloud:cloud)
        }
        observers["authorization"] = Task { [weak self] in
            for await value in ActivityAuthorizationInfo().activityEnablementUpdates {
                guard let self else {return};self.enabled=value
                await self.synchronize(foreground:false,force:true)
            }
        }
        observers["activities"] = Task { [weak self] in
            for await activity in Activity<WorkspaceActivityAttributes>.activityUpdates {self?.observe(activity)}
        }
        for activity in Activity<WorkspaceActivityAttributes>.activities {observe(activity)}
        if #available(iOS 17.2, *) {
            startToken=Activity<WorkspaceActivityAttributes>.pushToStartToken.map(hex)
            observers["start-token"] = Task { [weak self] in
                for await token in Activity<WorkspaceActivityAttributes>.pushToStartTokenUpdates {
                    guard let self else {return};self.startToken=self.hex(token);await self.synchronize(foreground:false,force:true)
                }
            }
        }
        Task {await synchronize(foreground:false,force:true)}
    }
    func tracks(machine:String,workspace:String) -> Bool {machineID==machine && registration?.workspaceID==workspace}
    func setTracking(_ value:Bool,machine:String,workspace:String) async {
        guard !busy,let send else {return}
        busy=true;defer {finishWork()}
        do {
            if let oldMachine=machineID,let old=registration {
                let state=try await send(oldMachine,.init(kind:"subscription",workspaceID:old.workspaceID)).subscription
                guard let state else {throw MobileError.invalidResponse}
                var removal=WorkspaceActivityRequest(kind:"set_subscription",workspaceID:old.workspaceID);removal.expectedRevision=state.revision
                _=try await replace(removal,machine:oldMachine)
                for activity in Activity<WorkspaceActivityAttributes>.activities where activity.attributes.machineID==oldMachine {await activity.end(nil,dismissalPolicy:.immediate)}
            }
            registration=nil;machineID=nil;defaults.removeObject(forKey:settingsKey)
            if value {
                guard enabled else {throw MobileError.unavailable}
                #if DEBUG
                let environment:APNsEnvironment = .sandbox
                #else
                let environment:APNsEnvironment = .production
                #endif
                var next=WorkspaceActivityRegistration(workspaceID:workspace,environment:environment,autoStart:true)
                if #available(iOS 17.2, *) {next.pushToStartToken=startToken}
                let response=try await send(machine,.init(kind:"subscription",workspaceID:workspace))
                guard let state=response.subscription else {throw MobileError.invalidResponse}
                var request=WorkspaceActivityRequest(kind:"set_subscription",workspaceID:workspace);request.expectedRevision=state.revision;request.registration=next
                _=try await replace(request,machine:machine)
                machineID=machine;registration=next;save()
            }
            error=nil
        } catch {self.error=error.localizedDescription}
        refreshNeeded=true
    }
    func synchronize(foreground:Bool,force:Bool=false) async {
        guard let machineID,let registration,let send else {return}
        if busy {refreshNeeded=true;return}
        guard force || refreshNeeded || Date.now.timeIntervalSince(lastRead)>=30 else {return}
        busy=true;refreshNeeded=false;defer {finishWork()}
        do {
            guard let state=try await send(machineID,.init(kind:"subscription",workspaceID:registration.workspaceID)).subscription else {throw MobileError.invalidResponse}
            // An old callback cannot re-enable a remotely disabled or replaced subscription.
            guard let remote=state.registration,remote.generation==registration.generation else {self.registration=nil;self.machineID=nil;defaults.removeObject(forKey:settingsKey);return}
            var next=remote
            next.dismissed = next.dismissed || registration.dismissed
            next.pushToStartToken=startToken
            let live=Activity<WorkspaceActivityAttributes>.activities.filter {a in a.attributes.machineID==machineID && a.attributes.workspaceID==registration.workspaceID && a.attributes.generation==registration.generation && a.activityState != .ended && a.activityState != .dismissed}
            if let activity=live.first {
                next.activityID=activity.id;next.cycleID=activity.attributes.cycleID;next.activityToken=activity.pushToken.map(hex)
                next.dismissed=activity.activityState == .dismissed
                observe(activity)
            } else if state.journal.endedAt != nil {
                next.activityToken=nil;next.activityID=nil;next.cycleID=nil
            }
            next.autoStart=enabled && !next.dismissed
            if !enabled {next.autoStart=false;next.pushToStartToken=nil;next.activityToken=nil}
            if next != state.registration {
                var update=WorkspaceActivityRequest(kind:"set_subscription",workspaceID:next.workspaceID);update.expectedRevision=state.revision;update.registration=next
                _=try await replace(update,machine:machineID);self.registration=next;save()
            }
            if foreground && enabled {
                if #available(iOS 17.2, *) {} else if live.isEmpty && !next.dismissed && state.journal.canStartForeground {
                    guard let summary=try await send(machineID,.init(kind:"summary",workspaceID:registration.workspaceID)).summary else {throw MobileError.invalidResponse}
                    if summary.content.active {
                        let attrs=WorkspaceActivityAttributes(machineID:machineID,workspaceID:registration.workspaceID,generation:registration.generation,cycleID:UUID().uuidString,startedAt:UInt64(Date.now.timeIntervalSince1970))
                        let activity=try Activity.request(attributes:attrs,content:ActivityContent(state:summary.content,staleDate:Date(timeIntervalSince1970:Double(summary.content.updatedAt)+120)),pushType:.token)
                        observe(activity);refreshNeeded=true
                    }
                }
            }
            lastRead = .now;error=nil
        } catch {
            self.error=error.localizedDescription
            if error as? MobileError == .unauthorized {
                for a in Activity<WorkspaceActivityAttributes>.activities where a.attributes.machineID==machineID {await a.end(nil,dismissalPolicy:.immediate)}
            }
        }
    }
    private func replace(_ request:WorkspaceActivityRequest,machine:String) async throws -> WorkspaceActivitySubscription {
        guard let send else {throw MobileError.unavailable}
        do {
            guard let state=try await send(machine,request).subscription,state.revision==request.expectedRevision.map({$0+1}),state.registration==request.registration else {throw MobileError.invalidResponse}
            return state
        } catch {
            let original=error
            // A lost mutation response is resolved by a read, never a repeated subscription write.
            guard let state=try? await send(machine,.init(kind:"subscription",workspaceID:request.workspaceID)).subscription,
                  state.revision==request.expectedRevision.map({$0+1}),state.registration==request.registration else {throw original}
            return state
        }
    }
    private func finishWork() {
        busy=false
        if refreshNeeded {Task {await synchronize(foreground:false,force:true)}}
    }
    private func observe(_ activity:Activity<WorkspaceActivityAttributes>) {
        guard activity.attributes.machineID==machineID,activity.attributes.generation==registration?.generation,observers[activity.id]==nil else {return}
        observers[activity.id]=Task { [weak self] in
            for await token in activity.pushTokenUpdates {
                guard let self else {return}
                guard self.registration?.generation==activity.attributes.generation else {return}
                self.registration?.activityToken=self.hex(token);self.registration?.activityID=activity.id;self.registration?.cycleID=activity.attributes.cycleID
                self.save();await self.synchronize(foreground:false,force:true)
            }
        }
        observers["state-"+activity.id]=Task { [weak self] in
            var ended=activity.activityState == .ended
            for await state in activity.activityStateUpdates {
                guard let self else {return}
                if state == .ended {ended=true}
                if state == .dismissed && !ended {
                    self.registration?.dismissed=true;self.save();await self.synchronize(foreground:false,force:true)
                }
            }
        }
    }
    private func hex(_ data:Data) -> String {data.map {String(format:"%02x",$0)}.joined()}
    private func save() {
        if let machineID,let registration,let data=try? JSONEncoder().encode(Settings(machine:machineID,registration:registration)) {defaults.set(data,forKey:settingsKey)}
    }
}
