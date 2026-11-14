#!/usr/bin/env python3
"""Two concurrent native question requests; answers must arrive in the next task prompt."""
import json
import os
import pathlib
import sys
import time


def send(value):
    print(json.dumps(value), flush=True)


def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)


while True:
    message = read()
    method, mid = message.get('method'), message.get('id')
    result = {}
    if method == 'initialize':
        assert 'form' in message['params']['clientCapabilities'].get('elicitation', {})
        result = {'protocolVersion': 1, 'agentCapabilities': {}}
    elif method == 'session/new':
        if os.environ.get('FAKE_QUESTION_FIRST', 'claude') == 'claude':
            options = message['params']['_meta']['claudeCode']['options']
            assert options['tools'] == ['Bash', 'Read', 'AskUserQuestion'], 'Staff must expose its native question tool'
            assert options['settings']['permissions']['ask'] == ['Bash', 'Read'], 'I/O must remain gated'
        result = {'sessionId': 'staff-session'}
    elif method == 'session/prompt':
        text = ''.join(part.get('text', '') for part in message['params']['prompt'])
        task = json.loads(text.split('\n任务快照：', 1)[1])
        if not any('Question' in item for item in task['messages']):
            fixtures = pathlib.Path(os.environ['FAKE_QUESTION_FIXTURES'])
            if os.environ.get('FAKE_QUESTION_PAUSE') == '1': time.sleep(.02)
            for index, provider in enumerate([os.environ.get('FAKE_QUESTION_FIRST', 'claude'), 'codex']):
                request = json.loads((fixtures / (provider + '-question.json')).read_text())
                request['id'] = 100 + index
                request['params']['toolCallId'] += str(index)
                send(request)
                if index == 0 and os.environ.get('FAKE_QUESTION_PAUSE') == '1': time.sleep(.08)
            replies = []
            while len(replies) < 2:
                reply = read()
                if reply.get('id') in [100, 101]:
                    replies.append(reply)
            pathlib.Path(os.environ['FAKE_QUESTION_LOG']).write_text(json.dumps(replies))
            assert all(reply.get('result') == {'action': 'decline'} for reply in replies)
            answer = 'Current turn continued without a tool answer.'
        else:
            user = [item['User']['text'] for item in task['messages'] if 'User' in item][-1]
            answer = 'NEXT TURN: ' + user
        send({'jsonrpc': '2.0', 'method': 'session/update', 'params': {
            'sessionId': 'staff-session', 'update': {'sessionUpdate': 'agent_message_chunk',
            'content': {'type': 'text', 'text': answer}}}})
        result = {'stopReason': 'end_turn'}
    if mid is not None:
        send({'jsonrpc': '2.0', 'id': mid, 'result': result})
