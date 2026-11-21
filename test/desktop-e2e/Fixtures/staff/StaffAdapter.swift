import Foundation

enum StaffAdapter {
    static let script = #"""
    #!/usr/bin/env python3
    import json, os, re, shlex, subprocess, sys, time
    root = os.environ['NORMA_BRAIN_DIR']
    def send(value): print(json.dumps(value), flush=True)
    def read():
        line = sys.stdin.readline()
        if not line: sys.exit(0)
        return json.loads(line)
    def record(name, value):
        with open(os.path.join(root, name), 'a') as f: f.write(json.dumps(value) + '\n')
    def update(sid, value):
        send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':value}})
    def hold(stage):
        record('stream-events.jsonl', dict(stage=stage))
        deadline = time.monotonic() + 900
        while not os.path.exists(os.path.join(root, stage)):
            if time.monotonic() > deadline: raise RuntimeError('Fixture release timed out: ' + stage)
            time.sleep(.05)
    counter = 100
    def run(session, argv):
        global counter
        counter += 1
        update(session, {'sessionUpdate':'tool_call','toolCallId':str(counter),'title':' '.join(argv[6:8]),'status':'in_progress','rawInput':{'command':shlex.join(argv)}})
        send({'jsonrpc':'2.0','id':counter,'method':'session/request_permission','params':{
            'sessionId':session,'toolCall':{'toolCallId':str(counter),'name':'Bash','kind':'execute','rawInput':{'command':shlex.join(argv)}},
            'options':[{'optionId':'yes','kind':'allow_once'},{'optionId':'no','kind':'reject_once'}]}})
        while True:
            answer = read()
            if answer.get('id') == counter: break
        allowed = answer.get('result',{}).get('outcome',{}).get('optionId') == 'yes'
        record('commands.jsonl', dict(argv=argv, allowed=allowed))
        if not allowed:
            update(session, {'sessionUpdate':'tool_call_update','toolCallId':str(counter),'status':'failed','rawOutput':'Permission gate declined this command.'})
            return None
        result = subprocess.run(argv, capture_output=True, text=True, timeout=40)
        record('results.jsonl', dict(argv=argv, code=result.returncode, output=result.stdout, error=result.stderr))
        update(session, {'sessionUpdate':'tool_call_update','toolCallId':str(counter),'status':'completed' if result.returncode == 0 else 'failed','rawOutput':result.stdout or result.stderr})
        if result.returncode: raise RuntimeError(result.stderr)
        return json.loads(result.stdout)
    def ask_cards(sid):
        forms = [
            {'theme': {'type':'string','title':'Which interface theme?', '_meta':{'codex':{'isOther':True,'isSecret':False}},
                'oneOf':[{'const':'Light','title':'Light','description':'Bright background'}, {'const':'Dark','title':'Dark','description':'Comfortable at night'}]},
             'checks': {'type':'array','title':'Which checks should Worker run?', '_meta':{'codex':{'isOther':True,'isSecret':False}},
                'items':{'anyOf':[{'const':'Keyboard','title':'Keyboard shortcuts'}, {'const':'Access','title':'Accessibility'}]}},
             'private_label': {'type':'string','title':'Private build label', '_meta':{'codex':{'isOther':True,'isSecret':True}}}},
            {'analytics': {'type':'string','title':'Enable optional analytics?', '_meta':{'codex':{'isOther':True,'isSecret':False}},
                'oneOf':[{'const':'Yes','title':'Yes'}, {'const':'No','title':'No'}]}}
        ]
        for index, fields in enumerate(forms):
            send({'jsonrpc':'2.0','id':900+index,'method':'elicitation/create','params':{'sessionId':sid,
                'toolCallId':'staff-question-'+str(index),'mode':'form','message':'Please choose',
                'requestedSchema':{'type':'object','properties':fields}}})
        replies = []
        while len(replies) < 2:
            answer = read()
            if answer.get('id') in [900,901]: replies.append(answer)
        record('question-replies.jsonl', {'replies':replies})
        assert all(item.get('result') == {'action':'decline'} for item in replies), replies
    memory = {}
    def load_task(text):
        # 同一会话的后续轮只带新增消息和状态变化，作用域与任务编号留在首轮。
        full = re.search(r'作用域：(\{.*\})\n任务快照：(\{.*\})', text)
        if full:
            scope, task = json.loads(full[1]), json.loads(full[2])
            binary = shlex.split(re.search(r'命令速查（都以 (.*?) brain 开头）', text)[1])[0]
            memory.update(scope=scope, task=task, binary=binary)
            return scope, task, binary
        if '新增消息：' not in text or 'task' not in memory:
            return None
        fresh = json.loads(text.split('新增消息：', 1)[1].split('\n状态变化：', 1)[0])
        task = memory['task']
        task.setdefault('messages', [])
        for item in fresh:
            task['messages'].append(json.loads(item) if isinstance(item, str) else item)
        if '\n状态变化：' in text:
            state = json.loads(text.split('\n状态变化：', 1)[1])
            for key in ('revision', 'phase', 'proposals', 'workers', 'questions'):
                if key in state: task[key] = state[key]
        return memory['scope'], task, memory['binary']

    while True:
        message = read(); method, mid = message.get('method'), message.get('id')
        if method == 'initialize': send({'jsonrpc':'2.0','id':mid,'result':{'protocolVersion':1,'agentCapabilities':{}}})
        elif method == 'session/new': send({'jsonrpc':'2.0','id':mid,'result':{'sessionId':'staff-shared'}})
        elif method in ('session/set_mode','session/set_model'): send({'jsonrpc':'2.0','id':mid,'result':{}})
        elif method == 'session/prompt':
            text = ''.join(p.get('text','') for p in message['params']['prompt'])
            record('prompts.jsonl', dict(text=text))
            sid = message['params']['sessionId']
            reply = ''
            # 富文本点击验收不需要 brain 命令，直接回复，不依赖提示词里的命令说明。
            rich_dir = os.environ.get('NORMA_E2E_RICH_LINK_DIR')
            if rich_dir and 'RICH-LINK' in text:
                reply = 'RICH-LINK 验收：打开 `' + rich_dir + '/rich-link.html`，笔记 `' + rich_dir + '/notes.md`，缺失 `' + rich_dir + '/missing.md`，外链 [Example](https://example.com)。'
                send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':reply}}}})
            # 长富文本验收：标题、含长路径的表格与列表、带软换行的长段落，滚到底必须完整可见。
            if 'LONG-REPLY' in text:
                root = '/Users/example/Documents/norma-ai-os'
                rows = ''.join('| 原型 %d | `%s/docs/prototypes/norma-long-path-prototype-number-%d-with-a-very-long-name.html` |\n' % (i, root, i) for i in range(1, 6))
                bullets = ''.join('- 条目 %d：`%s/docs/product-specs/norma-ios-prototypes-2026-09-%02d/overview-with-long-name.md`\n' % (i, root, i) for i in range(1, 6))
                reply = ('LONG-REPLY 验收\n\n## 原型文件\n\n| 文件 | 绝对路径 |\n| --- | --- |\n' + rows + '\n其他相关的：\n\n' + bullets +
                    '\n## 实施计划\n\n| 文件 | 绝对路径 |\n| --- | --- |\n' + rows + '\n另外还有 HTML 版计划，比如 `' + root + '/docs/exec-plans/2026-10-24-brain-propose-next-step.html`。\n和你这个富文本测试最贴的是幕僚架构设计稿：`' + root + '/docs/design-docs/2026-11-12-norma-staff-architecture.html`\n原因是消息行被撑成了很高的虚高，滚动按虚高算，首条就被顶出首屏。\n还没验收，是因为他只用布局边界测试验证过，没有在真实 App 里看过。\nLONG-REPLY END')
                update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':reply}})
            # 工具步骤路径验收：Read 带 locations，cd 只有命令，rg 命令里带查询路径。
            steps_dir = os.environ.get('NORMA_E2E_TOOL_STEPS_DIR')
            if steps_dir and 'TOOL-STEPS' in text:
                calls = [
                    ('s1', dict(title='Read', kind='read', locations=[dict(path=steps_dir + '/notes.md', line=3)], rawInput=dict(file_path=steps_dir + '/notes.md'))),
                    ('s2', dict(title='cd ' + steps_dir + '/docs/plans && ls', kind='execute', rawInput=dict(command='cd ' + steps_dir + '/docs/plans && ls'))),
                    ('s3', dict(title='rg', kind='search', rawInput=dict(command='rg -n needle docs/plans'))),
                ]
                for call_id, body in calls:
                    update(sid, dict(sessionUpdate='tool_call', toolCallId=call_id, status='in_progress', **body))
                    update(sid, dict(sessionUpdate='tool_call_update', toolCallId=call_id, status='completed', rawOutput='ok'))
                reply = 'TOOL-STEPS 验收：三步已完成。'
                update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':reply}})
            loaded = None if reply else load_task(text)
            if loaded:
                scope, task, binary = loaded
                base = [binary,'brain','--workspace',scope['workspace_id'],'--route',scope['route_id']]
                tid = task['id']
                users = [m['user'] for m in task['messages'] if 'user' in m]
                last = users[-1] if users else ''
                if 'PROPOSAL-LAYOUT' in last and not task['proposals']:
                    run(sid, base + ['task','propose',tid,'Claude','--title','归档不显示这条提示','--prompt','保持卡片完整可读。说明文字与建议卡分开排列，不修改归档提示逻辑。','--why','仅修复布局重叠，保留标题、理由、指令和交给谁。'])
                    reply = '归档成功那条提示不该再出现。我先看现在有哪些 Agent 能接着改 pr-235。截图里那条「已归档…仍在进行，可在已归档中查看」会整条去掉，撤销一起不显示。归档本身还在。交给 Ethan 在 pr-235 上改。已提议交给 Ethan，在 `/Users/openclawd/Documents/code/norma-os/pr-235` 里改：归档后不再弹出截图里那条「已归档…仍在进行，可在已归档中查看」，右边的撤销也一起去掉。\n\n归档本身还在，任务照样进已归档。你点采用就派给他。若只想删括号里那句、撤销还要留，跟我说一声再改。 LAYOUT END'
                elif last.startswith('CHAT-STOP '):
                    hold('chat-start')
                    update(sid, {'sessionUpdate':'tool_call','toolCallId':'chat-read','title':'Read conversation','status':'in_progress'})
                    update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':
                        'Skip this. Reply begins with **formatted words** that wrap naturally as the assistant explains this long piece of sample text with more detail than fits on a single line.\n\nReply ends here.'}})
                    record('stream-events.jsonl', dict(stage='chat-partial', pid=os.getpid()))
                    while True:
                        cancel = read()
                        if cancel.get('method') == 'session/cancel' and cancel['params']['sessionId'] == sid:
                            record('stream-events.jsonl', dict(stage='chat-cancelled'))
                            # Simulate a notification already queued by the adapter at cancellation.
                            update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'LATE OUTPUT'}})
                            send({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'cancelled'}})
                            break
                    continue
                elif last.startswith('Reply begins with formatted words'):
                    reply = 'SECOND TURN READY'
                elif 'STREAM-FAIL' in last:
                    send({'jsonrpc':'2.0','id':mid,'error':{'code':-32602,'message':'Fixture permanent failure'}})
                    continue
                elif 'STREAM-ALPHA' in last:
                    hold('release-waiting')
                    update(sid, {'sessionUpdate':'plan','entries':[
                        {'content':'Inspect Alpha project','priority':'high','status':'completed'},
                        {'content':'Prepare acceptance result','priority':'medium','status':'in_progress'}]})
                    update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'ALPHA PARTIAL: project inspected. '}})
                    # 本场景没有可用 Agent，用原生问答卡验证“卡片中途入库只往后追加”；不等待回答，轮次结束时由宿主按超时收口。
                    send({'jsonrpc':'2.0','id':950,'method':'elicitation/create','params':{'sessionId':sid,
                        'toolCallId':'staff-question-stream','mode':'form','message':'Please choose',
                        'requestedSchema':{'type':'object','properties':{'analytics': {'type':'string','title':'ALPHA QUESTION','_meta':{'codex':{'isOther':True,'isSecret':False}},
                            'oneOf':[{'const':'Yes','title':'Yes'}, {'const':'No','title':'No'}]}}}}})
                    hold('release-partial')
                    update(sid, {'sessionUpdate':'plan','entries':[
                        {'content':'Inspect Alpha project','priority':'high','status':'completed'},
                        {'content':'Prepare acceptance result','priority':'medium','status':'completed'}]})
                    update(sid, {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'ALPHA FINAL: acceptance ready.'}})
                    hold('release-final')
                elif 'STREAM-BETA' in last:
                    reply = 'BETA RESULT: separate project conversation.'
                elif os.environ.get('NORMA_E2E_STAFF_QUESTIONS') == '1' and not task['questions']:
                    ask_cards(sid)
                    reply = 'STAFF QUESTIONS: answers will be read in the next turn.'
                elif os.environ.get('NORMA_E2E_STAFF_ARCHIVE') == '1' and not task['proposals']:
                    run(sid, base + ['task','propose',tid,'Claude','--title','下一步','--prompt','继续 ARCHIVE-KEEP','--why','请确认'])
                    reply = 'STAFF PLAN：已提出下一步。'
                elif os.environ.get('NORMA_E2E_STAFF_ASSIGN') == '1' and 'WITHDRAW-NEW' in last and not task['proposals']:
                    run(sid, base + ['task','assign',tid,'Grace','--title','新开撤回','--prompt','echo withdraw-new','--why','验证新开的卡可以撤回'])
                    reply = 'STAFF WITHDRAW：已派一张新卡。'
                elif os.environ.get('NORMA_E2E_STAFF_ASSIGN') == '1' and 'WITHDRAW-OPEN' in last and not task['proposals']:
                    run(sid, base + ['task','assign',tid,'Grace','--title','先开一张','--prompt','echo withdraw-open','--why','先有一张可复用的卡'])
                    reply = 'STAFF WITHDRAW：先开了 Grace 的卡。'
                elif os.environ.get('NORMA_E2E_STAFF_ASSIGN') == '1' and 'WITHDRAW-AGAIN' in last:
                    run(sid, base + ['task','assign',tid,'Grace','--title','再派一次','--prompt','echo withdraw-again','--why','复用刚才那张卡'])
                    reply = 'STAFF WITHDRAW：再派给 Grace。'
                elif os.environ.get('NORMA_E2E_STAFF_ASSIGN') == '1' and 'NAMED-RISK' in last:
                    run(sid, base + ['task','assign',tid,'Grace','--title','点名发布','--prompt','git push origin main','--why','用户点名 Grace'])
                    reply = 'STAFF NAMED：已直接派给 Grace。'
                elif not task['proposals']:
                    roster = run(sid, base + ['agents'])
                    # Production CLI wraps Brain output in the response envelope.
                    agents = json.loads(roster['result']['Brain']['output'])['agents']
                    wanted = os.environ.get('NORMA_E2E_STAFF_AGENT')
                    enabled = [a for a in agents if a['enabled']]
                    agent = next(a['name'] for a in enabled if wanted and (a['name'] == wanted or a.get('profile') == wanted)) if wanted else enabled[0]['name']
                    run(sid, base + ['task','propose',tid,agent,'--title','实现验收文件','--prompt','STAFF-M1-WORK: 创建 STAFF-RESULT.md 并返回结果。','--why','先完成一个可验收产物'])
                    # Task sessions are forbidden to report: prove the permission gate rejects it.
                    run(sid, base + ['report','--text','TASK MUST NOT SPEAK ' + 'Long acceptance input. ' * 100])
                    reply = '**STAFF PLAN**：建议先生成验收文件。\n\n- 采用后开始\n- 产物：`STAFF-RESULT.md`'
                elif os.environ.get('NORMA_E2E_STAFF_ASSIGN') == '1' and 'ASSIGN-RISK' in last:
                    run(sid, base + ['task','assign',tid,task['workers'][0]['agent'],'--title','推送并发布','--prompt','git push origin main 然后发布','--why','收尾发布'])
                    reply = 'STAFF RISK：这条涉及推送，已交给你决定。'
                elif os.environ.get('NORMA_E2E_STAFF_ASSIGN') == '1' and 'WITHDRAW' not in task.get('title', '') and len(task['proposals']) == 1 and task['workers'] and (task['workers'][0].get('phase') or {}).get('Finished') == 'Succeeded':
                    run(sid, base + ['task','assign',tid,task['workers'][0]['agent'],'--title','补一条测试','--prompt','STAFF-HOLD 为验收文件补一条测试','--why','结果缺少测试'])
                    reply = 'STAFF ASSIGN：已把后续任务派给同一个 Agent。'
                elif 'CONFIRM' in last:
                    run(sid, base + ['task','phase',tid,'done'])
                    reply = 'STAFF DONE：验收完成。'
                elif 'MANUAL' in last:
                    run(sid, base + ['task','phase',tid,'merging'])
                    reply = 'STAFF MANUAL：保留你设置的阶段。'
                elif 'REOPEN' in last:
                    reply = 'STAFF REOPEN：继续处理新消息。'
                elif task['workers'] and (task['workers'][0].get('phase') or {}).get('Finished') == 'Succeeded':
                    run(sid, base + ['task','phase',tid,'merging'])
                    reply = 'STAFF REVIEW：结果已保存，请验收 STAFF-RESULT.md。'
                if reply:
                    send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':reply}}}})
            send({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'end_turn'}})
        elif mid is not None: send({'jsonrpc':'2.0','id':mid,'result':{}})
    """#
}
