import Foundation

/// 重放真实 codex-acp 2.2.2 的权限转换；只替换模型启动，保留 reporter/renderer。
public enum CodexPermissionFixture {
    public static func install() throws -> URL {
        let fm = FileManager.default
        let installed = ProcessInfo.processInfo.environment["NORMA_BRAIN_E2E_ADAPTERS"]
            .map { URL(fileURLWithPath: $0) }
            ?? fm.homeDirectoryForCurrentUser.appendingPathComponent(".norma/brain/adapters")
        let package = installed.appendingPathComponent("node_modules/@agentclientprotocol/codex-acp")
        let manifest = try Data(contentsOf: package.appendingPathComponent("package.json"))
        let identity = try JSONSerialization.jsonObject(with: manifest) as? [String: Any]
        guard identity?["version"] as? String == "2.2.2" else {
            throw BrainFixtureError.missingAdapter("需要 codex-acp 2.2.2")
        }
        let source = try String(contentsOf: package.appendingPathComponent("dist/index.js"), encoding: .utf8)
        guard source.components(separatedBy: "  startAcpServer();").count == 2 else {
            throw BrainFixtureError.missingAdapter("实际 bundle 入口不匹配")
        }
        let root = URL(fileURLWithPath: "/private/tmp/norma-codex-replay-\(UUID().uuidString)")
        let modules = root.appendingPathComponent("adapters/node_modules")
        let target = modules.appendingPathComponent("@agentclientprotocol/codex-acp")
        try fm.createDirectory(at: target.appendingPathComponent("dist"), withIntermediateDirectories: true)
        try fm.createDirectory(at: modules.appendingPathComponent(".bin"), withIntermediateDirectories: true)
        try manifest.write(to: target.appendingPathComponent("package.json"))
        let entry = target.appendingPathComponent("dist/index.js")
        try (source.replacingOccurrences(of: "  startAcpServer();", with: "  await normaReplay();") + replay)
            .write(to: entry, atomically: true, encoding: .utf8)
        try fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: entry.path)
        try fm.createSymbolicLink(at: modules.appendingPathComponent(".bin/codex-acp"), withDestinationURL: entry)
        try #"{"enabled":true,"backend":"codex","strategy":"speed","speak":false,"summary":"off"}"#
            .write(to: root.appendingPathComponent("config.json"), atomically: true, encoding: .utf8)
        return root
    }

    private static let replay = #"""

    async function normaReplay() {
      const { createInterface } = await import('node:readline');
      const { appendFileSync } = await import('node:fs');
      const { spawnSync } = await import('node:child_process');
      const lines = createInterface({ input: process.stdin })[Symbol.asyncIterator]();
      const send = value => process.stdout.write(JSON.stringify({jsonrpc:'2.0', ...value}) + '\n');
      const read = async () => { const next = await lines.next(); return next.done ? null : JSON.parse(next.value); };
      const log = value => appendFileSync(process.env.NORMA_BRAIN_DIR + '/replay.jsonl', JSON.stringify(value) + '\n');
      const renderer = new AcpToolCallRenderer({airClient:false, air:{}});
      const sid = 'codex-permission-replay-' + process.pid;
      let done = false, rid = 700;
      async function permission(command, started) {
        const id = ++rid;
        const facts = CommandReporter.permission({itemId:'replay-'+id, command}, started, started ? 'exec_command' : undefined);
        const call = renderer.renderPermissionToolCall(facts);
        send({id, method:'session/request_permission', params:{sessionId:sid, toolCall:call,
          options:[{optionId:'yes',kind:'allow_once',name:'Allow'},{optionId:'no',kind:'reject_once',name:'Deny'}]}});
        for (;;) {
          const answer = await read();
          if (!answer) return false;
          if (answer.id === id) {
            const allowed = answer.result.outcome.optionId === 'yes';
            log({session:sid, command, call, allowed});
            return allowed;
          }
        }
      }
      for (;;) {
        const m = await read(); if (!m) break;
        if (m.method === 'initialize') send({id:m.id,result:{protocolVersion:1,agentCapabilities:{}}});
        else if (m.method === 'session/new') send({id:m.id,result:{sessionId:sid}});
        else if (m.method === 'session/prompt') {
          const text = m.params.prompt.map(x=>x.text||'').join('');
          log({session:sid,prompt:text});
          const bin = text.match(/`([^`\s]+) brain agents`/)?.[1];
          if (bin && !done) {
            done = true;
            const body = `${bin} brain report --text 'Codex quote replay delivered' --priority now`;
            const command = `/bin/zsh -lc "${body}"`;
            if (await permission(command, false)) {
              const result = spawnSync('/bin/zsh',['-lc',body],{encoding:'utf8',env:process.env});
              log({session:sid,reportExit:result.status,stdout:result.stdout,stderr:result.stderr});
            }
            await permission(`/bin/zsh -lc '/bin/echo forbidden-replay'`, true);
            await permission(`/bin/zsh -lc '${bin} brain agents; /bin/echo forbidden-compound'`, true);
          }
          send({method:'session/update',params:{sessionId:sid,update:{sessionUpdate:'agent_message_chunk',content:{type:'text',text:'Codex permission replay finished'}}}});
          send({id:m.id,result:{stopReason:'end_turn'}});
        } else if (m.id !== undefined) send({id:m.id,result:{}});
      }
    }
    """#
}
