import SwiftUI
import ActivityKit
import NormaCore

@MainActor @Observable final class WorkspaceActivityManager {
    private(set) var registrations: [String: WorkspaceActivityRegistration] = [:]
    private(set) var enabled = ActivityAuthorizationInfo().areActivitiesEnabled
    private(set) var busy = false
    var error: String?
    private var startToken: String?
    private var send: ((String, WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse)?
    private var pairedMachines: (() async -> [String])?
    private var knownMachines: [String] = []
    private var observers: [String: Task<Void, Never>] = [:]
    private var refreshNeeded = false
    private var forgotten: Set<String> = []
    private var pendingRemoval: [String: @Sendable () async throws -> Void] = [:]
    private var removing: Set<String> = []
    private var lastRead = Date.distantPast
    private let defaults: UserDefaults
    private let settingsKey = "workspace-live-activities"
    private struct LegacySettings: Codable {let machine: String; let registration: WorkspaceActivityRegistration}

    init(defaults: UserDefaults = .standard, send: ((String, WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse)? = nil) {
        self.defaults=defaults; self.send=send
        if let data=defaults.data(forKey:settingsKey),let saved=try? JSONDecoder().decode([String:WorkspaceActivityRegistration].self,from:data) {registrations=saved}
        else if let data=defaults.data(forKey:"workspace-live-activity"),let old=try? JSONDecoder().decode(LegacySettings.self,from:data) {registrations[old.machine]=old.registration}
    }
    func attach(workspaces: WorkspaceChatModel, computers: ComputerModel, cloud: CloudChatModel) {
        guard observers["authorization"]==nil else {return}
        if send==nil {send={machine,request in
            if !computers.loaded {await computers.load()}
            return try await workspaces.activity(machineID:machine,request:request,computers:computers,cloud:cloud)
        }}
        pairedMachines={
            if !computers.loaded {await computers.load()}
            return computers.machines.map(\.id)
        }
        computers.pairedMachinesChanged={ [weak self] in Task {await self?.synchronize(foreground:false,force:true)} }
        observers["authorization"]=Task { [weak self] in
            for await value in ActivityAuthorizationInfo().activityEnablementUpdates {
                guard let self else {return};self.enabled=value
                await self.synchronize(foreground:false,force:true)
            }
        }
        observers["activities"]=Task { [weak self] in
            for await activity in Activity<WorkspaceActivityAttributes>.activityUpdates {self?.observe(activity)}
        }
        for activity in Activity<WorkspaceActivityAttributes>.activities {observe(activity)}
        if #available(iOS 17.2, *) {
            startToken=Activity<WorkspaceActivityAttributes>.pushToStartToken.map(hex)
            observers["start-token"]=Task { [weak self] in
                for await token in Activity<WorkspaceActivityAttributes>.pushToStartTokenUpdates {
                    guard let self else {return};self.startToken=self.hex(token)
                    await self.synchronize(foreground:false,force:true)
                }
            }
        }
        Task {await synchronize(foreground:false,force:true)}
    }
    func synchronize(foreground: Bool, force: Bool=false, machines: [String]?=nil) async {
        guard let send else {return}
        if busy {refreshNeeded=true;return}
        busy=true;defer {finishWork()}
        let current:[String]
        if let machines {current=machines}
        else if let pairedMachines {current=await pairedMachines()}
        else {current=knownMachines}
        let changed=Set(current) != Set(knownMachines)
        knownMachines=current
        forgotten=forgotten.filter {current.contains($0) || pendingRemoval[$0] != nil}
        guard force || changed || refreshNeeded || Date.now.timeIntervalSince(lastRead)>=30 else {return}
        refreshNeeded=false
        for machine in registrations.keys.filter({!current.contains($0)}) {await forget(machine)}
        var failure:String?
        // Sequential enrollment uses the existing bounded LAN/relay requests, without a socket burst.
        for machine in current where !forgotten.contains(machine) && pendingRemoval[machine]==nil {
            do {
                guard let state=try await send(machine,.init(kind:"subscription")).subscription else {throw MobileError.invalidResponse}
                guard !forgotten.contains(machine) else {continue}
                var next=state.registration ?? registrations[machine] ?? makeRegistration()
                if next.workspaceID != nil {
                    for activity in Activity<WorkspaceActivityAttributes>.activities where activity.attributes.machineID==machine {await activity.end(nil,dismissalPolicy:.immediate)}
                    next=makeRegistration()
                }
                if state.registration==nil && !enabled {continue}
                if let local=registrations[machine],local.generation==next.generation,
                   !state.journal.quietSeen,(state.journal.cycleID==nil || local.cycleID==state.journal.cycleID) {next.dismissed = next.dismissed || local.dismissed}
                let live=Activity<WorkspaceActivityAttributes>.activities.filter {a in
                    a.attributes.machineID==machine && a.attributes.generation==next.generation &&
                    a.activityState != .ended && a.activityState != .dismissed &&
                    !state.journal.quietSeen &&
                    (state.journal.cycleID==nil || a.attributes.cycleID==state.journal.cycleID)
                }
                if let activity=live.first {
                    next.activityID=activity.id;next.cycleID=activity.attributes.cycleID;next.activityToken=activity.pushToken.map(hex)
                } else if state.journal.endedAt != nil {
                    next.activityToken=nil;next.activityID=nil;next.cycleID=nil
                }
                next.autoStart=enabled
                next.pushToStartToken=enabled ? startToken:nil
                if !enabled {next.activityToken=nil}
                // Persist the intended generation before a write; an uncertain reply cannot create another one.
                guard !forgotten.contains(machine) else {continue}
                registrations[machine]=next;save()
                if next != state.registration {
                    var update=WorkspaceActivityRequest(kind:"set_subscription");update.expectedRevision=state.revision;update.registration=next
                    _=try await replace(update,machine:machine)
                }
                guard !forgotten.contains(machine) else {continue}
                for activity in live {observe(activity)}
                if foreground && enabled {
                    if #available(iOS 17.2, *) {} else if live.isEmpty && !next.dismissed && state.journal.canStartForeground {
                        guard let summary=try await send(machine,.init(kind:"summary")).summary else {throw MobileError.invalidResponse}
                        if summary.content.active {
                            let attrs=WorkspaceActivityAttributes(machineID:machine,generation:next.generation,cycleID:UUID().uuidString,startedAt:UInt64(Date.now.timeIntervalSince1970))
                            let activity=try Activity.request(attributes:attrs,content:ActivityContent(state:summary.content,staleDate:Date(timeIntervalSince1970:Double(summary.content.updatedAt)+120)),pushType:.token)
                            registrations[machine]?.activityID=activity.id;registrations[machine]?.cycleID=attrs.cycleID;save()
                            observe(activity)
                        }
                    }
                }
            } catch {
                failure=error.localizedDescription
                if error as? MobileError == .unauthorized {await endActivities(machine)}
            }
        }
        error=failure;lastRead = .now
    }
    func forget(_ machine:String,cleanup:(@Sendable () async throws -> Void)?=nil) async {
        forgotten.insert(machine)
        registrations[machine]=nil;save()
        pendingRemoval[machine]=cleanup ?? {}
        await endActivities(machine)
        if !busy {finishWork()}
    }
    private func endActivities(_ machine:String) async {
        for activity in Activity<WorkspaceActivityAttributes>.activities where activity.attributes.machineID==machine {
            observers.removeValue(forKey:activity.id)?.cancel()
            observers.removeValue(forKey:"state-"+activity.id)?.cancel()
            await activity.end(nil,dismissalPolicy:.immediate)
        }
    }
    private func makeRegistration() -> WorkspaceActivityRegistration {
        #if DEBUG
        let environment:APNsEnvironment = .sandbox
        #else
        let environment:APNsEnvironment = .production
        #endif
        return WorkspaceActivityRegistration(environment:environment,autoStart:true)
    }
    private func replace(_ request:WorkspaceActivityRequest,machine:String) async throws -> WorkspaceActivitySubscription {
        guard let send else {throw MobileError.unavailable}
        do {
            guard let state=try await send(machine,request).subscription,state.revision==request.expectedRevision.map({$0+1}),state.registration==request.registration else {throw MobileError.invalidResponse}
            return state
        } catch {
            let original=error
            guard let state=try? await send(machine,.init(kind:"subscription")).subscription,
                  state.revision==request.expectedRevision.map({$0+1}),state.registration==request.registration else {throw original}
            return state
        }
    }
    private func finishWork() {
        busy=false
        for (machine,cleanup) in pendingRemoval where !removing.contains(machine) {
            removing.insert(machine)
            Task {
                do {try await cleanup()}
                catch {self.error="此电脑的远端锁屏登记未能清理，请在 Mac 上撤销此手机授权。"}
                pendingRemoval[machine]=nil;removing.remove(machine)
            }
        }
        if refreshNeeded {Task {await synchronize(foreground:false,force:true)}}
    }
    private func observe(_ activity:Activity<WorkspaceActivityAttributes>) {
        let machine=activity.attributes.machineID
        guard activity.attributes.generation==registrations[machine]?.generation,observers[activity.id]==nil else {return}
        observers[activity.id]=Task { [weak self] in
            for await token in activity.pushTokenUpdates {
                guard let self else {return}
                guard self.registrations[machine]?.generation==activity.attributes.generation else {return}
                if let cycle=self.registrations[machine]?.cycleID,cycle != activity.attributes.cycleID {return}
                self.registrations[machine]?.activityToken=self.hex(token);self.registrations[machine]?.activityID=activity.id;self.registrations[machine]?.cycleID=activity.attributes.cycleID
                self.save();await self.synchronize(foreground:false,force:true)
            }
        }
        observers["state-"+activity.id]=Task { [weak self] in
            var ended=activity.activityState == .ended
            for await state in activity.activityStateUpdates {
                guard let self else {return}
                if state == .ended {ended=true}
                if state == .dismissed && !ended,
                   self.registrations[machine]?.generation==activity.attributes.generation,
                   self.registrations[machine]?.cycleID==activity.attributes.cycleID {
                    self.registrations[machine]?.dismissed=true;self.save();await self.synchronize(foreground:false,force:true)
                }
            }
        }
    }
    private func hex(_ data:Data) -> String {data.map {String(format:"%02x",$0)}.joined()}
    private func save() {
        if let data=try? JSONEncoder().encode(registrations) {defaults.set(data,forKey:settingsKey);defaults.removeObject(forKey:"workspace-live-activity")}
    }
}
