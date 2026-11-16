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
            match = None if reply else re.search(r'作用域：(\{.*\})\n任务快照：(\{.*\})', text)
            if match:
                scope, task = json.loads(match[1]), json.loads(match[2])
                binary = shlex.split(re.search(r'只能执行 (.*?) brain 命令', text)[1])[0]
                base = [binary,'brain','--workspace',scope['workspace_id'],'--route',scope['route_id']]
                tid = task['id']
                users = [m['user'] for m in task['messages'] if 'user' in m]
                last = users[-1] if users else ''
                if 'PROPOSAL-LAYOUT' in last and not task['proposals']:
                    run(sid, base + ['task','propose',tid,'Claude','--title','归档不显示这条提示','--prompt','保持卡片完整可读。说明文字与建议卡分开排列，不修改归档提示逻辑。','--why','仅修复布局重叠，保留标题、理由、指令和交给谁。'])
                    reply = '归档成功那条提示不该再出现。我先看现在有哪些 Agent 能接着改 pr-235。截图里那条「已归档…仍在进行，可在已归档中查看」会整条去掉，撤销一起不显示。归档本身还在。交给 Ethan 在 pr-235 上改。已提议交给 Ethan，在 `/Users/openclawd/Documents/code/norma-os/pr-235` 里改：归档后不再弹出截图里那条「已归档…仍在进行，可在已归档中查看」，右边的撤销也一起去掉。\n\n归档本身还在，任务照样进已归档。你点采用就派给他。若只想删括号里那句、撤销还要留，跟我说一声再改。 LAYOUT END'
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
                elif os.environ.get('NORMA_E2E_STAFF_ASSIGN') == '1' and len(task['proposals']) == 1 and task['workers'] and task['workers'][0].get('outcome') == 'Succeeded':
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
                elif task['workers'] and task['workers'][0].get('outcome') == 'Succeeded':
                    run(sid, base + ['task','phase',tid,'merging'])
                    reply = 'STAFF REVIEW：结果已保存，请验收 STAFF-RESULT.md。'
                if reply:
                    send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':sid,'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':reply}}}})
            send({'jsonrpc':'2.0','id':mid,'result':{'stopReason':'end_turn'}})
        elif mid is not None: send({'jsonrpc':'2.0','id':mid,'result':{}})
    """#
}
