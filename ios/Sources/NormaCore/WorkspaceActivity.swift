import Foundation

public struct WorkspaceActivityLink: Hashable, Sendable, Identifiable {
    public let machineID: String
    public let workspaceID: String
    public let areaID: String?
    public var id: String { url.absoluteString }
    public init(machineID: String, workspaceID: String, areaID: String? = nil) { self.machineID=machineID;self.workspaceID=workspaceID;self.areaID=areaID }
    public init?(url: URL) {
        guard url.scheme=="norma",url.host=="activity", let c=URLComponents(url:url,resolvingAgainstBaseURL:false),
              let items=c.queryItems,Set(items.map(\.name)).count==items.count,
              items.allSatisfy({["machine","workspace","area"].contains($0.name)}),
              let machine=items.first(where:{$0.name=="machine"})?.value, machine.hasPrefix("mac-"),validHex(String(machine.dropFirst(4))),
              let workspace=items.first(where:{$0.name=="workspace"})?.value,validID(workspace) else {return nil}
        let area=items.first(where:{$0.name=="area"})?.value
        guard area==nil || validID(area!) else {return nil}
        self.init(machineID:machine,workspaceID:workspace,areaID:area)
    }
    public var url: URL {
        var c=URLComponents();c.scheme="norma";c.host="activity"
        c.queryItems=[.init(name:"machine",value:machineID),.init(name:"workspace",value:workspaceID)]
        if let areaID {c.queryItems?.append(.init(name:"area",value:areaID))}
        return c.url!
    }
}
public struct WorkspaceActivityTarget: Codable, Equatable, Sendable {
    public let areaID: String;public let nodeID: UInt64;public let profile: String
    public let runID: String;public let sessionID: String;public let turnID: String
    public let requestID: String?;public let fingerprint: String?
    enum CodingKeys: String,CodingKey {case areaID="area_id",nodeID="node_id",profile,runID="run_id",sessionID="session_id",turnID="turn_id",requestID="request_id",fingerprint}
    public func validate() throws {
        guard validID(areaID),nodeID>0,["Codex","Claude","Cursor"].contains(profile),[runID,sessionID,turnID].allSatisfy(validID),
              fingerprint==nil || validHex(fingerprint!) else {throw MobileError.invalidResponse}
        if let requestID {
            let target=ApprovalTarget(nodeID:nodeID,profile:profile,runID:runID,sessionID:sessionID,turnID:turnID,requestID:requestID)
            try target.validate()
        }
    }
    public func matches(_ a: ApprovalTarget) -> Bool {a.nodeID==nodeID && a.profile==profile && a.runID==runID && a.sessionID==sessionID && a.turnID==turnID && a.requestID==requestID}
}
public struct WorkspaceActivityContent: Codable, Hashable, Sendable {
    public let name: String; public let updatedAt: UInt64;public let active: Bool
    public let runningCount: Int;public let approvalCount: Int;public let reviewCount: Int;public let failureCount: Int;public let hiddenProjects: Int
    public let projects: [Project]
    enum CodingKeys: String,CodingKey {case name,updatedAt="updated_at",active,runningCount="running_count",approvalCount="approval_count",reviewCount="review_count",failureCount="failure_count",hiddenProjects="hidden_projects",projects}
    public struct Project: Codable, Hashable, Sendable, Identifiable {
        public let areaID: String;public let name: String;public let status: String;public let reason: String;public let agents: [Agent];public let otherAgents: Int
        public var id: String {areaID}
        enum CodingKeys: String,CodingKey {case areaID="area_id",name,status,reason,agents,otherAgents="other_agents"}
        public var statusLabel: String {switch status {case "approval":"待审批";case "failed":"失败";case "review":"待验收";case "running":"执行中";default:"空闲"}}
    }
    public struct Agent: Codable, Hashable, Sendable {
        public let name: String;public let action: String
        enum CodingKeys: String,CodingKey {case name,action,state,progress}
        private struct Progress: Decodable {let currentStep: String?;enum CodingKeys:String,CodingKey {case currentStep="current_step"}}
        public init(from decoder: any Decoder) throws {
            let c=try decoder.container(keyedBy:CodingKeys.self);name=try c.decode(String.self,forKey:.name)
            action=try c.decodeIfPresent(String.self,forKey:.action) ?? c.decodeIfPresent(Progress.self,forKey:.progress)?.currentStep ?? c.decodeIfPresent(String.self,forKey:.state) ?? ""
        }
        public func encode(to encoder: any Encoder) throws {var c=encoder.container(keyedBy:CodingKeys.self);try c.encode(name,forKey:.name);try c.encode(action,forKey:.action)}
    }
    public func isStale(at date: Date = .now) -> Bool {date.timeIntervalSince1970 >= Double(updatedAt)+120}
    public func validate() throws {
        guard name.utf8.count<=60,updatedAt>0,[runningCount,approvalCount,reviewCount,failureCount,hiddenProjects].allSatisfy({$0>=0}),projects.count<=3,Set(projects.map(\.id)).count==projects.count,
              projects.allSatisfy({validID($0.areaID) && $0.name.utf8.count<=48 && $0.reason.utf8.count<=120 && ["approval","failed","review","running","idle"].contains($0.status) && $0.agents.count<=2 && $0.otherAgents>=0 && $0.agents.allSatisfy({$0.name.utf8.count<=30 && $0.action.utf8.count<=60})}),try JSONEncoder().encode(self).count<=3500 else {throw MobileError.invalidResponse}
    }
}
public struct WorkspaceActivitySummary: Decodable, Sendable {
    public let machineID: String;public let workspaceID: String;public let content: WorkspaceActivityContent;public let attention: [Attention]
    public struct Attention: Decodable, Sendable, Identifiable {
        public let target: WorkspaceActivityTarget;public let kind: String;public let project: String;public let agent: String;public let reason: String
        public var id: String {"\(target.areaID)/\(target.nodeID)/\(target.runID)/\(target.turnID)/\(target.requestID ?? target.fingerprint ?? kind)"}
    }
    enum CodingKeys:String,CodingKey {case machineID="machine_id",workspaceID="desktop_workspace_id",attention}
    public init(from decoder: any Decoder) throws {
        let c=try decoder.container(keyedBy:CodingKeys.self);machineID=try c.decode(String.self,forKey:.machineID);workspaceID=try c.decode(String.self,forKey:.workspaceID);attention=try c.decode([Attention].self,forKey:.attention);content=try WorkspaceActivityContent(from:decoder)
    }
}
public struct WorkspaceActivityRegistration: Codable, Equatable, Sendable, CustomDebugStringConvertible {
    public var workspaceID: String;public var generation: String;public var environment: APNsEnvironment;public var autoStart: Bool
    public var pushToStartToken: String?;public var activityToken: String?;public var activityID: String?;public var cycleID: String?;public var dismissed: Bool
    public init(workspaceID:String,generation:String=UUID().uuidString,environment:APNsEnvironment,autoStart:Bool) {self.workspaceID=workspaceID;self.generation=generation;self.environment=environment;self.autoStart=autoStart;dismissed=false}
    public var debugDescription:String {"WorkspaceActivityRegistration(tokens: [redacted])"}
    enum CodingKeys:String,CodingKey {case workspaceID="desktop_workspace_id",generation,environment,autoStart="auto_start",pushToStartToken="push_to_start_token",activityToken="activity_token",activityID="activity_id",cycleID="cycle_id",dismissed}
    public func validate() throws {
        guard validID(workspaceID),validID(generation),activityToken==nil || (activityID != nil && cycleID != nil) else {throw MobileError.invalidResponse}
        for id in [activityID,cycleID].compactMap({$0}) {guard validID(id) else {throw MobileError.invalidResponse}}
        for token in [pushToStartToken,activityToken].compactMap({$0}) {try APNsRegistration(token:token,environment:environment).validate()}
    }
}
public struct WorkspaceActivitySubscription: Decodable, Sendable {
    public let revision: UInt64;public let registration: WorkspaceActivityRegistration?;public let journal: Journal
    public struct Journal: Decodable, Sendable {
        public let cycleID:String?;public let startedAt:UInt64?;public let endedAt:UInt64?;public let quietSeen:Bool
        public var canStartForeground:Bool {startedAt==nil || (endedAt != nil && quietSeen)}
        enum CodingKeys:String,CodingKey {case cycleID="cycle_id",startedAt="started_at",endedAt="ended_at",quietSeen="quiet_seen"}
    }
}
public struct WorkspaceActivityRequest: Codable, Sendable {
    public var kind: String;public let workspaceID:String
    public var target:WorkspaceActivityTarget?;public var offset:Int?;public var operationID:String?;public var decision:ApprovalDecision?;public var areaID:String?;public var expectedRevision:UInt64?;public var registration:WorkspaceActivityRegistration?
    public init(kind:String,workspaceID:String) {self.kind=kind;self.workspaceID=workspaceID}
    public var isMutation:Bool {["set_subscription","decide","accept"].contains(kind)}
    enum CodingKeys:String,CodingKey {case kind,workspaceID="desktop_workspace_id",target,offset,operationID="operation_id",decision,areaID="area_id",expectedRevision="expected_revision",registration}
}
public struct WorkspaceActivityResponse: Decodable, Sendable {
    public let summary:WorkspaceActivitySummary?;public let subscription:WorkspaceActivitySubscription?;public let target:WorkspaceActivityTarget?;public let approval:ApprovalSnapshot?;public let delivery:Delivery?;public let receipt:MobileCommandReceipt?;public let project:DesktopChatProject?
    public struct Delivery:Decodable,Sendable {public let offset:Int;public let total:Int;public let chunk:String;public let accepted:Bool;public let completed:Bool}
}
#if canImport(ActivityKit) && os(iOS)
import ActivityKit
public struct WorkspaceActivityAttributes: ActivityAttributes {
    public typealias ContentState=WorkspaceActivityContent
    public let machineID:String;public let workspaceID:String;public let generation:String;public let cycleID:String;public let startedAt:UInt64
    public init(machineID:String,workspaceID:String,generation:String,cycleID:String,startedAt:UInt64) {self.machineID=machineID;self.workspaceID=workspaceID;self.generation=generation;self.cycleID=cycleID;self.startedAt=startedAt}
    enum CodingKeys:String,CodingKey {case machineID="machine_id",workspaceID="desktop_workspace_id",generation,cycleID="cycle_id",startedAt="started_at"}
}
#endif
