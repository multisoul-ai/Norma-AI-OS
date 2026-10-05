import SwiftUI
import NormaCore

struct WorkspaceActivityDetailView: View {
    let link:WorkspaceActivityLink
    let workspaces:WorkspaceChatModel
    let computers:ComputerModel
    let cloud:CloudChatModel
    var onClose:(() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss
    @State private var summary:WorkspaceActivitySummary?
    @State private var selected:WorkspaceActivitySummary.Attention?
    @State private var approval:ApprovalSnapshot?
    @State private var review:ProjectReview?
    @State private var failure:String?
    @State private var canAccept=false
    @State private var busy=false
    @State private var error:String?
    @State private var outcome:String?
    @State private var pending:[WorkspaceActivityDecisionStore.Pending]=[]
    private let decisions=WorkspaceActivityDecisionStore(url:URL.applicationSupportDirectory.appendingPathComponent("Norma/activity-decisions.json"))
    var body:some View {
        NavigationStack {
            List {
                if let summary {
                    Section(summary.content.name) {
                        Text("\(summary.content.runningCount) 执行 · \(summary.content.approvalCount) 审批 · \(summary.content.reviewCount) 验收")
                        ForEach(summary.attention.filter {link.areaID==nil || $0.target.areaID==link.areaID}) {item in
                            Button {Task {await load(item)}} label: {
                                VStack(alignment:.leading,spacing:5) {Text("\(item.project) · \(item.agent)").font(.headline);Text(item.reason).font(.subheadline).foregroundStyle(.secondary)}.frame(minHeight:44)
                            }.accessibilityIdentifier("activity-item-\(item.id)").disabled(busy)
                        }
                        if summary.attention.isEmpty {Text("当前没有待处理事项")}
                        ForEach(summary.content.projects.filter {link.areaID==nil || $0.areaID==link.areaID}) {project in
                            VStack(alignment:.leading,spacing:5) {Text("\(project.name) · \(project.statusLabel)").font(.headline);Text(project.reason).foregroundStyle(.secondary)}
                            ForEach(Array(project.agents.enumerated()),id:\.offset) {_,agent in Text("\(agent.name) · \(agent.action)").font(.subheadline).foregroundStyle(.secondary)}
                        }
                    }
                }
                if let selected,let approval {
                    Section("\(selected.agent) · \(approval.tool)") {
                        Text((try? approval.details.formatted()) ?? "无法读取审批原文").font(.system(.footnote,design:.monospaced)).textSelection(.enabled)
                        ForEach(approval.decisions,id:\.self) {decision in
                            Button(decision == .allow ? "允许这次工具调用" : "拒绝这次工具调用",role:decision == .deny ? .destructive:nil) {Task {await decide(selected,choice:decision)}}
                                .frame(minHeight:44).disabled(busy || hasPending(selected.target)).accessibilityIdentifier("activity-decision-\(decision.rawValue)")
                        }
                    }
                }
                if let selected,let failure {Section("\(selected.agent) · 执行失败") {Text(failure).textSelection(.enabled)}}
                if let selected,let review {
                    Section("冻结成果 · \(selected.agent)") {
                        ForEach(review.files) {file in
                            NavigationLink {ReviewFileView(file:file,baselineName:"任务起点",resultName:"任务成果",showsIndex:false)} label: {Label(file.path,systemImage:"doc.text")}
                        }
                        if review.files.isEmpty {Text("该成果没有文件变更")}
                        Button("通过验收") {Task {await decide(selected,choice:nil)}}.frame(minHeight:44).disabled(!canAccept || busy || hasPending(selected.target)).accessibilityIdentifier("activity-accept")
                    }
                }
                if !pending.isEmpty {
                    Section("未决操作") {
                        ForEach(pending.filter {$0.machineID==link.machineID && $0.request.workspaceID==link.workspaceID}) {item in
                            Button("查询原操作结果 · \(item.request.kind == "accept" ? "验收" : "审批")") {Task {await query(item)}}.frame(minHeight:44).disabled(busy)
                        }
                        Text("响应丢失时只查询原操作；请在 Mac 核对仍未知的结果。").font(.footnote).foregroundStyle(.secondary)
                    }
                }
                if let outcome {Text(outcome).foregroundStyle(.secondary)}
                if let error {Text(error).foregroundStyle(.red)}
                if busy {ProgressView("正在向 Mac 核验")}
            }.navigationTitle("工作空间详情").navigationBarTitleDisplayMode(.inline)
                .toolbar {ToolbarItem(placement:.cancellationAction) {Button("完成") {if let onClose {onClose()} else {dismiss()}}};ToolbarItem(placement:.topBarTrailing) {Button("刷新") {Task {await refresh()}}.disabled(busy)}}
                .task {await refresh()}.refreshable {await refresh()}
        }.tint(IdeaPalette.blue).preferredColorScheme(.light)
    }
    private func request(_ request:WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse {try await workspaces.activity(machineID:link.machineID,request:request,computers:computers,cloud:cloud)}
    private func refresh() async {
        guard !busy else {return};busy=true;defer {busy=false}
        do {pending=try await decisions.load();summary=try await request(.init(kind:"summary",workspaceID:link.workspaceID)).summary;error=nil}
        catch {self.error=error.localizedDescription;approval=nil;canAccept=false;failure=nil}
    }
    private func load(_ item:WorkspaceActivitySummary.Attention) async {
        guard !busy else {return};busy=true;defer {busy=false}
        approval=nil;review=nil;failure=nil;canAccept=false;selected=item;outcome=nil
        do {
            var read=WorkspaceActivityRequest(kind:"detail",workspaceID:link.workspaceID);read.target=item.target
            if item.kind == "failed" {
                guard let project=try await request(read).project,
                      let agent=project.agents.first(where:{$0.card.nodeID==item.target.nodeID && $0.card.profile==item.target.profile && $0.card.runID==item.target.runID && $0.card.sessionID==item.target.sessionID && $0.card.turnID==item.target.turnID}),agent.card.state=="failed" else {throw MobileError.invalidResponse}
                failure=agent.card.taskSummary ?? item.reason
            } else if item.kind == "approval" {approval=try await request(read).approval}
            else if item.kind == "review" {
                var data=Data();var total:Int?;var accepted=false;var completed=false
                while total==nil || data.count<total! {
                    read.offset=data.count
                    guard let response=try await request(read).delivery,response.offset==data.count,response.total<=32*1024*1024,response.total>=data.count,(total==nil || total==response.total),!response.chunk.isEmpty || response.total==0 else {throw MobileError.invalidResponse}
                    data.append(contentsOf:response.chunk.utf8);guard data.count<=response.total else {throw MobileError.invalidResponse}
                    total=response.total;accepted=response.accepted;completed=response.completed
                }
                struct Content:Decodable {let status:String;let review:ProjectReview?}
                let content=try JSONDecoder().decode(Content.self,from:data)
                guard content.status=="available",let result=content.review else {throw MobileError.invalidResponse}
                try result.validateContent();review=result;canAccept=completed && !accepted && !result.conflicted
            }
            error=nil
        } catch {self.error=error.localizedDescription;approval=nil;review=nil;canAccept=false}
    }
    private func hasPending(_ target:WorkspaceActivityTarget) -> Bool {pending.contains {$0.machineID==link.machineID && $0.request.workspaceID==link.workspaceID && $0.request.target==target}}
    private func decide(_ item:WorkspaceActivitySummary.Attention,choice:ApprovalDecision?) async {
        guard !busy,!hasPending(item.target) else {return};busy=true;defer {busy=false}
        var prepared:WorkspaceActivityDecisionStore.Pending?
        var submitted=false
        do {
            var write=WorkspaceActivityRequest(kind:choice==nil ? "accept":"decide",workspaceID:link.workspaceID);write.target=item.target;write.operationID=UUID().uuidString;write.decision=choice
            let original=WorkspaceActivityDecisionStore.Pending(machineID:link.machineID,request:write)
            prepared=original
            try await decisions.save(original);pending=try await decisions.load()
            // Re-fetch exact facts immediately before submission; the Mac repeats this check at dispatch.
            var read=WorkspaceActivityRequest(kind:"detail",workspaceID:link.workspaceID);read.target=item.target
            _=try await request(read)
            submitted=true
            guard let receipt=try await request(write).receipt else {throw MobileError.invalidResponse}
            try await apply(receipt,original:original);error=nil;approval=nil;canAccept=false
            summary=try await request(.init(kind:"summary",workspaceID:link.workspaceID)).summary
        } catch {
            if !submitted,let prepared {try? await decisions.remove(prepared.id);pending=(try? await decisions.load()) ?? pending}
            self.error=submitted ? "\(error.localizedDescription)；请查询原操作结果。":error.localizedDescription
        }
    }
    private func query(_ original:WorkspaceActivityDecisionStore.Pending) async {
        guard !busy else {return};busy=true;defer {busy=false}
        do {
            var read=WorkspaceActivityRequest(kind:"receipt",workspaceID:original.request.workspaceID);read.operationID=original.id;read.areaID=original.request.target?.areaID
            guard let receipt=try await request(read).receipt else {throw MobileError.invalidResponse}
            try await apply(receipt,original:original);error=nil
        } catch {self.error=error.localizedDescription}
    }
    private func apply(_ receipt:MobileCommandReceipt,original:WorkspaceActivityDecisionStore.Pending) async throws {
        try original.validateReceipt(receipt)
        if ["succeeded","rejected"].contains(receipt.state) {try await decisions.remove(original.id)}
        pending=try await decisions.load()
        outcome=receipt.state=="succeeded" ? "Mac 已确认处理成功":receipt.state=="rejected" ? "Mac 已拒绝：\(receipt.error ?? "目标已变化")":"操作结果尚未确认，请在 Mac 核对。"
    }
}
