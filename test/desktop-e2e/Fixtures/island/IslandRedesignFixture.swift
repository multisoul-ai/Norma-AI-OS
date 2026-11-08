import E2EKit
import Foundation

/// 真实画布快照 + 会话检查点。假 CLI 只占用 PATH，经生产 Hook 端点上报，不写岛的内部状态。
/// Codex / OpenCode 的版本探针故意不是产品钉死的版本，于是走原生命令而不是 app-server。
/// Sam 用 Claude 的 AskUserQuestion：Codex 提问要真实 app-server 中继，这条夹具接不上。
public enum IslandRedesignFixture {
    public static func install() throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp").appendingPathComponent("norma-island-redesign-\(UUID().uuidString)")
        let bin = root.appendingPathComponent("bin")
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        try Data("export PATH=\"\(bin.path):$PATH\"\nexport NORMA_ISLAND_FIXTURE=\"\(root.path)\"\n".utf8)
            .write(to: root.appendingPathComponent(".zshrc"))
        let script = Data(agent.utf8)
        for name in ["claude", "codex", "cursor-agent", "opencode", "kimi"] {
            let path = bin.appendingPathComponent(name)
            try script.write(to: path)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        }
        return root
    }

    /// 只放入本阶段的前 `count` 个角色。8 人时 Daisy 在画布上但不恢复进程，所以是排队。
    public static func seed(database: URL, count: Int) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-c", seeder, database.path, String(count)]
        let pipe = Pipe()
        process.standardError = pipe
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            let err = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            throw E2EError.product("岛夹具写入失败 \(process.terminationStatus): \(err)")
        }
    }

    public static func finishLeo(_ root: URL) throws {
        try Data().write(to: root.appendingPathComponent("finish-leo"))
    }

    private static let seeder = #"""
import hashlib, json, pathlib, sqlite3, sys, time
db = sqlite3.connect(sys.argv[1])
count = int(sys.argv[2])
root = pathlib.Path(sys.argv[1]).parent
# 上一阶段留下的定义仍占用 Leo 等名字。画布被重置成无 definition_id 的旧卡后，
# 迁移会因重名改派名称池里的下一个名字。每个阶段开始时清掉项目定义。
defs = root / 'desktop-project-definitions'
if defs.exists():
    for path in defs.glob('*.sqlite'):
        con = sqlite3.connect(path)
        for table in ('agent_dispatch_tasks', 'agent_conversation_tails', 'agent_definitions'):
            try:
                con.execute('delete from ' + table)
            except sqlite3.OperationalError:
                pass
        con.commit()
        con.close()
people = [
    (1,'Grace','Claude','claude','Norma-AI-OS','run'),
    (2,'Iris','Claude','claude','Norma-AI-OS','approval'),
    (3,'Kevin','Codex','codex','Norma-AI-OS','run'),
    (4,'Leo','OpenCode','opencode','norma-cloud','done'),
    (5,'Mia','Cursor','cursor','multisoul','run'),
    (6,'Sam','Claude','claude','multisoul','question'),
    (7,'Noah','OpenCode','opencode','norma-cloud','run'),
    (8,'Daisy','Kimi','kimi','Norma-AI-OS','queued'),
][:count]
now = int(time.time() * 1000)
areas = {}
# 创建时间相同再按 id 排序时，ws-multisoul 会排在 ws-norma-cloud 前面。
# 按夹具里第一次出现的顺序拉开时间，分组才是 Norma-AI-OS、norma-cloud、multisoul。
for index, ws in enumerate(dict.fromkeys(p[4] for p in people)):
    created = now + index
    row = db.execute('select id from desktop_workspaces where name=?', (ws,)).fetchone()
    if row is None:
        wid = 'ws-' + ws
        db.execute('insert into desktop_workspaces(id,name,created_at_unix_ms,last_opened_at_unix_ms) values(?,?,?,?)', (wid, ws, created, created))
    else:
        wid = row[0]
        db.execute('update desktop_workspaces set created_at_unix_ms=? where id=?', (created, wid))
    area = db.execute('select id, root_path from desktop_project_areas where desktop_workspace_id=? and display_name=?', (wid, ws)).fetchone()
    if area is None:
        aid, path = 'area-' + ws, str(root / 'projects' / ws)
        pathlib.Path(path).mkdir(parents=True, exist_ok=True)
        db.execute('insert into desktop_project_areas(id,desktop_workspace_id,project_key,root_path,display_name,added_at_unix_ms) values(?,?,?,?,?,?)', (aid, wid, path, path, ws, now))
    else:
        aid, path = area
    areas[ws] = (wid, aid, path)
db.commit()
canvases = root / 'desktop-canvases'
canvases.mkdir(exist_ok=True)
by_ws = {}
for person in people:
    by_ws.setdefault(person[4], []).append(person)
for ws, group in by_ws.items():
    wid, aid, _ = areas[ws]
    name = hashlib.sha256(f'{wid}:{aid}'.encode()).hexdigest() + '.sqlite'
    con = sqlite3.connect(canvases / name)
    con.execute('create table if not exists canvas_snapshots(id integer primary key check(id=1), json text not null, updated_at_unix integer not null)')
    con.execute('create table if not exists session_checkpoints(node_id integer primary key not null, kind text not null, payload text not null, cols integer not null default 80, rows integer not null default 24, shell text, agent_profile text, updated_at_unix integer not null)')
    nodes = []
    con.execute('delete from session_checkpoints')
    for index, (node, alias, profile, hook, _, role) in enumerate(group):
        nodes.append({'id': node, 'kind': {'Agent': {'profile': profile, 'alias': alias}}, 'bounds': {'origin': {'x': float(index * 680), 'y': 0}, 'size': {'x': 640, 'y': 420}}, 'z_index': node})
        if role != 'queued':
            con.execute('insert into session_checkpoints(node_id,kind,payload,cols,rows,shell,agent_profile,updated_at_unix) values(?,?,?,?,?,?,?,?)', (node, 'agent', alias, 100, 32, None, hook, int(time.time())))
    doc = {'nodes': nodes, 'viewport': {'pan': {'x': 0, 'y': 0}, 'zoom': 1}}
    con.execute('insert or replace into canvas_snapshots values(1,?,?)', (json.dumps(doc), int(time.time())))
    con.commit()
if count >= 4:
    wid, aid, path = areas['norma-cloud']
    scope = f'desktop:{wid}:{aid}'
    project = pathlib.Path(path)
    encode = lambda value: json.dumps(value, ensure_ascii=False, separators=(',', ':'))
    names = ['relay.mjs', 'retry.mjs', 'relay.test.mjs']
    files = []
    for name in names:
        text = 'relay ' + name + '\n'
        (project / name).write_text(text)
        files.append(dict(path=name, status='??', before=None, after=dict(kind='text', executable=False, bytes=list(text.encode())), index=None, diff=None, staged_diff=None))
    key = dict(node_id=4, profile='OpenCode', run_id='run-leo', session_id='session-leo', turn_id='turn-leo')
    outcome = dict(key=key, state='completed')
    content = dict(status='available', review=dict(base_commit=None, files=files, conflicted=False))
    fingerprint = hashlib.sha256(encode([scope, outcome, content]).encode()).hexdigest()
    saved = dict(workspace_id=scope, outcome=outcome, content=content, fingerprint=fingerprint, saved_at_unix=int(time.time()))
    db.execute('delete from mobile_turn_outcomes where run_id=?', ('run-leo',))
    db.execute('delete from mobile_task_deliveries where run_id=?', ('run-leo',))
    db.execute('insert into mobile_turn_outcomes values(?,?,?,?,?)', (scope, 'run-leo', 'turn-leo', encode(outcome), int(time.time())))
    db.execute('insert into mobile_task_deliveries(workspace_id,run_id,turn_id,data_json,node_id) values(?,?,?,?,?)', (scope, 'run-leo', 'turn-leo', encode(saved), 4))
    db.commit()
"""#

    private static let agent = #"""
#!/usr/bin/env python3
import json, os, sys, time, urllib.request
argv = sys.argv[1:]
name = os.path.basename(sys.argv[0])
if '--version' in argv:
    print('codex-cli 0.0.0-e2e' if name == 'codex' else '0.0.0-e2e')
    raise SystemExit(0)
if 'app-server' in argv or 'serve' in argv:
    time.sleep(3600)
root = os.environ.get('NORMA_ISLAND_FIXTURE', '')
node = os.environ.get('MULTISOUL_NODE_ID', '')
def log(text):
    try:
        open(root + '/agent.log', 'a').write(text + '\n')
    except OSError:
        pass
if not node:
    log(name + ' no-node ' + ' '.join(argv))
    time.sleep(3600)
roster = {
    '1': ('run', '支持打开项目外部文件', '调用 view_image'),
    '2': ('approval', 'Git worktree 清理', '清理增量编译缓存'),
    '3': ('run', '灵动岛原型', '编辑 island/mod.rs'),
    '4': ('done', 'push relay 失败重试', '3 个文件'),
    '5': ('run', 'iOS 发布流水线', 'cargo test'),
    '6': ('question', '配对二维码刷新', '二维码多久刷新一次？'),
    '7': ('run', '推送重试单测', 'npm test'),
}
role, task, step = roster[node]
profile = {'claude': 'Claude', 'codex': 'Codex', 'cursor': 'Cursor', 'opencode': 'OpenCode', 'kimi': 'Kimi'}[os.environ['MULTISOUL_AGENT_PROFILE']]
endpoint = os.environ['MULTISOUL_HOOK_ENDPOINT']
token = os.environ['MULTISOUL_HOOK_TOKEN']
session = 'island-' + node

def post(path, payload, timeout):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(endpoint + path, data=data, method='POST')
    req.add_header('Authorization', 'Bearer ' + token)
    req.add_header('Content-Type', 'application/json')
    req.add_header('Content-Length', str(len(data)))
    with urllib.request.urlopen(req, timeout=timeout) as res:
        log(node + ' ' + path + ' ' + str(res.status))

def event(kind, tool=None, tool_input=None, extra=None):
    raw = {'hook_event_name': kind, 'session_id': session, 'prompt_id': 'turn-' + node, 'prompt': task}
    if tool:
        raw['tool_name'] = tool
        raw['tool_use_id'] = 'tool-' + node
        raw['tool_input'] = tool_input or {}
    if extra:
        raw.update(extra)
    body = {
        'node_id': int(node), 'profile': profile, 'provider_event': kind,
        'session_id': session, 'turn_id': 'turn-' + node, 'tool_name': tool,
        'message': None, 'raw_json': raw,
    }
    post('', body, 10)

try:
    event('SessionStart')
    event('UserPromptSubmit')
    if role == 'approval':
        command = 'rm -rf target/debug/incremental'
        tool_input = {'command': command, 'description': step}
        sys.stdout.write('\x1b[2J\x1b[HBash command\r\n  ' + command + '\r\n  ' + step + '\r\nDo you want to proceed?\r\n  1. Yes\r\n  2. No\r\nEsc to cancel · Tab to amend · ctrl+e to explain\r\n')
        sys.stdout.flush()
        event('PreToolUse', 'Bash', tool_input)
        event('PermissionRequest', 'Bash', tool_input)
    elif role == 'question':
        tool_input = {'questions': [{'question': step, 'options': [{'label': '每 30 秒'}, {'label': '每 60 秒'}]}]}
        raw = {'hook_event_name': 'PreToolUse', 'tool_name': 'AskUserQuestion', 'session_id': session, 'prompt_id': 'turn-' + node, 'tool_use_id': 'tool-' + node, 'tool_input': tool_input}
        body = {'run_id': os.environ.get('NORMA_CONTROL_RUN_ID') or ('q-' + node), 'event': {
            'node_id': int(node), 'profile': 'Claude', 'provider_event': 'PreToolUse',
            'session_id': session, 'turn_id': 'turn-' + node, 'tool_name': 'AskUserQuestion',
            'message': None, 'raw_json': raw,
        }}
        open(root + '/ready-' + node, 'w').close()
        while True:
            try:
                post('/question', body, 130)
            except Exception as error:
                log(node + ' question-retry ' + repr(error))
                time.sleep(0.3)
    else:
        event('PreToolUse', 'Bash', {'command': step, 'description': step})
        open(root + '/ready-' + node, 'w').close()
        print('ISLAND LIVE ' + node + ' ' + task, flush=True)
        if role == 'done':
            while not os.path.exists(root + '/finish-leo'):
                time.sleep(0.1)
            event('Stop')
            log(node + ' stopped')
    if role != 'question':
        open(root + '/ready-' + node, 'w').close()
except Exception as error:
    log(node + ' ERROR ' + repr(error))
    open(root + '/failed-' + node, 'w').write(repr(error))
while True:
    time.sleep(1)
"""#
}
