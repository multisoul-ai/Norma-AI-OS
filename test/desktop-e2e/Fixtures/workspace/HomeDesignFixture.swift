import E2EKit
import Foundation

public enum HomeDesignFixture {
    public static let names = ["Norma OS", "官网改版", "iOS 伴侣 App", "推送中继", "语音识别实验", "设计系统", "数据看板", "文档站", "客户端重构", "性能调优", "发布流水线", "插件市场"]

    /// 调用方已停止隔离App；确定访问时间以验证最近四个与更早折叠。
    public static func install(database: URL, count: Int, theme: String, alternate: Bool) throws {
        try Catalog.suppressFeatureGuide(database: database)
        try SQLRaw.execute(database: database, statements: [
            ("PRAGMA foreign_keys = ON", []),
            ("DELETE FROM desktop_project_areas", []),
            ("DELETE FROM desktop_workspaces", []),
        ])
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        for i in 0..<count {
            let age = i < 4 ? Int64(i) * 60_000 : Int64(i) * 86_400_000
            try SQLRaw.execute(database: database, statements: [(
                "INSERT INTO desktop_workspaces(id,name,created_at_unix_ms,last_opened_at_unix_ms) VALUES(?1,?2,?3,?4)",
                ["home-\(i)", names[i], String(now - Int64(12 - i) * 60_000), String(now - age)]
            )])
            if i < 4 {
                for area in 0..<(i % 3 + 1) {
                    let root = database.deletingLastPathComponent().appendingPathComponent("home-project-\(i)-\(area)")
                    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
                    try Catalog.seedArea(database: database, workspaceName: names[i], root: root, displayName: "Project \(area + 1)")
                }
            }
        }
        try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: theme)
        try Catalog.seedSetting(database: database, key: "wallpaper.kind", value: "builtin")
        try Catalog.seedSetting(database: database, key: "wallpaper.value", value: alternate ? "wallpapers/norma-blue-horizon.png" : "home/earth-horizon.png")
        try Catalog.seedSetting(database: database, key: "ui.language.v1", value: "zh-CN")
    }

    /// 只写隔离数据：前三种真实布局分别为左大右两小、左右双窗、单窗。
    /// App 必须已经停止；按正式 desktop canvas 路径与快照格式写入，重启走正常读取。
    public static func seedPreviewNodes(database: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-c", #"""
import hashlib, json, pathlib, sqlite3, sys
database = pathlib.Path(sys.argv[1])
catalog = sqlite3.connect(database)
root = database.parent / 'desktop-canvases'
root.mkdir(exist_ok=True)
layouts = [[(-100,-60,400,400),(320,-60,200,180),(320,160,200,180)],
           [(0,0,400,320),(420,0,400,320)], [(0,0,500,320)]]
for workspace, area, name in catalog.execute('select desktop_workspace_id,id,display_name from desktop_project_areas'):
    index = int(name.split()[-1])-1
    nodes = [{'id':i+1,'kind':{'Term':None},
              'bounds':{'origin':{'x':x,'y':y},'size':{'x':w,'y':h}},'z_index':i+1}
             for i,(x,y,w,h) in enumerate(layouts[index % 3])]
    file = root / (hashlib.sha256((workspace+':'+area).encode()).hexdigest()+'.sqlite')
    db = sqlite3.connect(file)
    db.execute('create table if not exists canvas_snapshots(id integer primary key check(id=1), json text not null, updated_at_unix integer not null)')
    doc = {'nodes':nodes, 'viewport':{'pan':{'x':0,'y':0},'zoom':1}}
    db.execute('insert or replace into canvas_snapshots values(1,?,0)', (json.dumps(doc),))
    db.commit()
"""#, database.path]
        process.standardOutput = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw E2EError.product("示意预览节点夹具写入失败") }
    }
}
