import Foundation

/// Persist before submitting. Unknown decisions are queried by the same operation, never replayed.
public actor WorkspaceActivityDecisionStore {
    public struct Pending: Codable, Sendable, Identifiable {
        public let machineID:String;public let request:WorkspaceActivityRequest
        public var id:String {request.operationID ?? ""}
        public init(machineID:String,request:WorkspaceActivityRequest) {self.machineID=machineID;self.request=request}
        public func validateReceipt(_ receipt:MobileCommandReceipt) throws {
            guard let target=request.target,receipt.operationID==id,receipt.workspaceID=="desktop:\(request.workspaceID):\(target.areaID)", ["accepted","succeeded","rejected","outcome_unknown"].contains(receipt.state) else {throw MobileError.invalidResponse}
            if receipt.state=="succeeded" {
                guard receipt.error==nil,let result=receipt.result,result.nodeID==target.nodeID,result.runID==target.runID,result.sessionID==target.sessionID,result.turnID==target.turnID else {throw MobileError.invalidResponse}
                if request.kind=="decide" {guard result.requestID==target.requestID,result.decision==request.decision else {throw MobileError.invalidResponse}}
                if request.kind=="accept" {guard result.fingerprint==target.fingerprint else {throw MobileError.invalidResponse}}
            }
        }
    }
    private let url:URL
    public init(url:URL) {self.url=url}
    public func load() throws -> [Pending] {
        guard FileManager.default.fileExists(atPath:url.path) else {return []}
        let data=try Data(contentsOf:url)
        guard data.count<=256*1024 else {throw MobileError.invalidResponse}
        let values=try JSONDecoder().decode([Pending].self,from:data)
        guard values.count<=128,Set(values.map(\.id)).count==values.count else {throw MobileError.invalidResponse}
        for value in values {try validate(value)}
        return values
    }
    public func save(_ pending:Pending) throws {
        try validate(pending);var values=try load()
        guard !values.contains(where:{$0.id==pending.id}),values.count<128 else {throw MobileError.commandConflict}
        values.append(pending);try write(values)
    }
    public func remove(_ id:String) throws {try write(load().filter {$0.id != id})}
    private func validate(_ pending:Pending) throws {
        guard pending.machineID.hasPrefix("mac-"),validHex(String(pending.machineID.dropFirst(4))),validID(pending.request.workspaceID),validID(pending.id),["decide","accept"].contains(pending.request.kind),let target=pending.request.target else {throw MobileError.invalidResponse}
        try target.validate()
        guard pending.request.kind != "decide" || (target.requestID != nil && pending.request.decision != nil),pending.request.kind != "accept" || target.fingerprint != nil else {throw MobileError.invalidResponse}
    }
    private func write(_ values:[Pending]) throws {
        let data=try JSONEncoder().encode(values);guard data.count<=256*1024 else {throw MobileError.invalidResponse}
        try FileManager.default.createDirectory(at:url.deletingLastPathComponent(),withIntermediateDirectories:true)
        #if os(iOS)
        try data.write(to:url,options:[.atomic,.completeFileProtectionUntilFirstUserAuthentication])
        #else
        try data.write(to:url,options:.atomic)
        #endif
    }
}
