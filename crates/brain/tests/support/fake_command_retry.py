"""Replay OpenCode's rejection -> cancelled turn; continuation must contain host feedback."""
import json, os, sys

def send(value):
    print(json.dumps(value), flush=True)

def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)

def update(body):
    send(dict(jsonrpc='2.0', method='session/update', params=dict(sessionId='retry-session', update=body)))

def permission(command, ident):
    send(dict(jsonrpc='2.0', id=ident, method='session/request_permission', params=dict(
        sessionId='retry-session', toolCall=dict(toolCallId=str(ident), name='Bash', kind='execute', rawInput=dict(command=command)),
        options=[dict(optionId='yes', kind='allow_once'), dict(optionId='no', kind='reject_once')])))
    response = read()
    assert response['id'] == ident
    return response.get('result', {}).get('outcome', {}).get('optionId')

commands = ["/opt/norma brain --workspace w --route r task phase t active", "/opt/norma brain --workspace w --route r task propose t Chloe --title Review --prompt 'Review the code' --why 'Independent review'"]
case = os.environ.get('RETRY_CASE', 'recover')
if case == 'prior-success':
    commands[0] = "/opt/norma brain --workspace w --route r task reply t --text 'Already sent'"
turn = 0
while True:
    msg = read()
    method, mid = msg.get('method'), msg.get('id')
    if method == 'initialize':
        send(dict(jsonrpc='2.0', id=mid, result=dict(protocolVersion=1, agentCapabilities={})))
    elif method == 'session/new':
        send(dict(jsonrpc='2.0', id=mid, result=dict(sessionId='retry-session')))
    elif method == 'session/prompt':
        turn += 1
        text = ''.join(p.get('text', '') for p in msg['params']['prompt'])
        with open(os.environ['RETRY_LOG'], 'a') as f:
            f.write(json.dumps(dict(turn=turn, prompt=text))+'\n')
        if case == 'prior-success' and turn == 1:
            assert permission(commands[0], 99) == 'yes'
        if turn == 1 or case == 'repeat':
            rejected = ' && '.join(commands) if case != 'unsafe' else commands[0] + ' && /bin/true'
            assert permission(rejected, 100) == 'no'
            if case == 'partial':
                assert permission(commands[0], 101) == 'yes'
            if case == 'natural':
                for index, command in enumerate(commands):
                    assert permission(command, 110+index) == 'yes'
                update(dict(sessionUpdate='agent_message_chunk', content=dict(type='text', text='RECOVERED')))
                send(dict(jsonrpc='2.0', id=mid, result=dict(stopReason='end_turn')))
                continue
            if case == 'late-end':
                import time
                time.sleep(.15)
                send(dict(jsonrpc='2.0', id=mid, result=dict(stopReason='cancelled')))
                continue
            if case == 'timeout':
                import time
                time.sleep(2)
            update(dict(sessionUpdate='tool_call_update', toolCallId='100', status='failed', rawOutput='The user rejected permission to use this specific tool call.'))
            update(dict(sessionUpdate='agent_message_chunk', content=dict(type='text', text='User rejected.')))
            send(dict(jsonrpc='2.0', id=mid, result=dict(stopReason='cancelled')))
        else:
            assert 'command format' in text.lower() and '&&' in text
            remaining = commands
            if case in ('partial', 'prior-success'):
                data = json.loads(text.split('\n')[-1])
                assert data['unexecuted_commands'] == [commands[1]], data
                remaining = commands[1:]
            for index, command in enumerate(remaining):
                assert permission(command, 200+index) == 'yes'
            update(dict(sessionUpdate='agent_message_chunk', content=dict(type='text', text='RECOVERED')))
            send(dict(jsonrpc='2.0', id=mid, result=dict(stopReason='end_turn')))
    elif mid is not None:
        send(dict(jsonrpc='2.0', id=mid, result={}))
