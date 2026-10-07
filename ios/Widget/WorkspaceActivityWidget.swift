import ActivityKit
import WidgetKit
import SwiftUI
import NormaCore

@main struct NormaWidgets: WidgetBundle {var body:some Widget {WorkspaceActivityWidget()}}
struct WorkspaceActivityWidget: Widget {
    var body:some WidgetConfiguration {
        ActivityConfiguration(for:WorkspaceActivityAttributes.self) {context in
            VStack(alignment:.leading,spacing:5) {
                HStack {
                    Text(context.state.name).font(.headline).lineLimit(1)
                    Spacer(minLength:4)
                    if context.state.hiddenProjects>0 {Text("另\(context.state.hiddenProjects)个项目").font(.caption2)}
                    if !context.state.active {Text("已结束").font(.caption)}
                    else if context.isStale {Label("状态已过期",systemImage:"clock.badge.exclamationmark").font(.caption)}
                }
                HStack(spacing:14) {
                    Label("\(context.state.runningCount) 执行",systemImage:"person.2")
                    Label("\(context.state.approvalCount) 审批",systemImage:"hand.raised")
                    Label("\(context.state.reviewCount) 验收",systemImage:"checkmark.circle")
                }.font(.caption.weight(.semibold)).accessibilityElement(children:.combine)
                ForEach(context.state.projects) {project in
                    Link(destination:projectURL(project,attributes:context.attributes)) {
                        HStack(alignment:.top,spacing:8) {
                            Text(project.statusLabel).font(.caption.weight(.semibold)).foregroundStyle(color(project.status)).frame(width:48,alignment:.leading)
                            VStack(alignment:.leading,spacing:1) {
                                Text(project.name + " · " + project.agents.map(\.name).joined(separator:" / ")).font(.caption.weight(.semibold)).lineLimit(1)
                                Text([project.workspaceName,detail(project)].compactMap {$0}.filter {!$0.isEmpty}.joined(separator:" · ")).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                            }
                            Spacer(minLength:0);Image(systemName:"chevron.right").font(.caption2)
                        }.accessibilityElement(children:.combine)
                    }.foregroundStyle(.primary)
                }
                if !context.state.active && context.state.projects.isEmpty {Text("本轮工作已结束 · 待处理事项继续在收件箱展示").font(.caption2).foregroundStyle(.secondary)}
            }.padding(12).activityBackgroundTint(Color.black.opacity(0.06))
                .widgetURL(URL(string:"norma://inbox")!)
        } dynamicIsland: {context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {Label("\(context.state.runningCount)",systemImage:"person.2")}
                DynamicIslandExpandedRegion(.trailing) {Text("\(context.state.approvalCount) 审批 · \(context.state.reviewCount) 验收")}
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment:.leading,spacing:4) {
                        Text(context.state.name).font(.headline)
                        if !context.state.active {Text("本轮工作已结束 · 打开收件箱").font(.caption)}
                        else if context.isStale {Text("状态已过期，打开 App 核验").font(.caption)}
                        else {Text(context.state.projects.first?.reason ?? "打开工作空间").font(.caption).lineLimit(2)}
                    }
                }
            } compactLeading: {Image(systemName:"person.2.fill")}
              compactTrailing: {Text("\(context.state.approvalCount+context.state.reviewCount)").monospacedDigit()}
              minimal: {Text("\(context.state.approvalCount+context.state.reviewCount)").monospacedDigit()}
                .widgetURL(URL(string:"norma://inbox")!)
        }
    }
    private func projectURL(_ project:WorkspaceActivityContent.Project,attributes:WorkspaceActivityAttributes) -> URL {
        guard let workspace=project.workspaceID ?? attributes.workspaceID else {return URL(string:"norma://inbox")!}
        return WorkspaceActivityLink(machineID:attributes.machineID,workspaceID:workspace,areaID:project.areaID).url
    }
    private func color(_ status:String) -> Color {switch status {case "approval":.orange;case "failed":.red;case "review":.green;default:.secondary}}
    private func detail(_ project:WorkspaceActivityContent.Project) -> String {
        if project.status=="running" {return project.agents.map {project.agents.count==1 ? action($0.action):"\($0.name) · \(action($0.action))"}.joined(separator:" / ")}
        if let name=project.agents.first?.name,project.reason.hasPrefix(name+" · ") {return String(project.reason.dropFirst(name.count+3))}
        return project.reason
    }
    private func action(_ state:String) -> String {switch state {case "thinking":"思考中";case "running_tool":"执行工具";case "preparing":"准备中";case "collecting":"收集成果";default:state}}
}
