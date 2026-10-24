"""Use a local API fixture and the real adapter; no provider calls or credentials.

The baseline must omit Read permissions, while production session options must
request permission even for an ordinary file in cwd. All requested tools are
rejected by this client. Requires an installed Claude ACP adapter and Node.
"""
import argparse
import http.server
import json
import os
import queue
import secrets
import signal
import subprocess
import tempfile
import threading
import time
import pathlib
parser = argparse.ArgumentParser()
parser.add_argument('--adapter', required=True)
parser.add_argument('--node', required=True)
parser.add_argument('--options-json', required=True)
args = parser.parse_args()
ADAPTER = args.adapter

def run(ask):
    with tempfile.TemporaryDirectory(prefix='norma-read-probe-') as d:
        target = str(pathlib.Path(d) / 'safe.txt')
        marker = 'fixture-' + secrets.token_hex(16)
        pathlib.Path(target).write_text(marker)
        calls = []
        tool_results = []

        class Handler(http.server.BaseHTTPRequestHandler):

            def log_message(self, *a):
                pass

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
                if 'count_tokens' in self.path:
                    payload = json.dumps({'input_tokens': 100}).encode()
                    self.send_response(200)
                    self.end_headers()
                    self.wfile.write(payload)
                    return
                calls.append(self.path)
                results = [c for m in body.get('messages', []) if m.get('role') == 'user' and isinstance(m.get('content'), list) for c in m['content'] if c.get('type') == 'tool_result' and c.get('tool_use_id') == 'read_probe_1']
                tool_results.extend(results)
                result = bool(results)
                content = [{'type': 'text', 'text': 'done'}] if result else [{'type': 'tool_use', 'id': 'read_probe_1', 'name': 'Read', 'input': {'file_path': target}}]
                stop = 'end_turn' if result else 'tool_use'
                message = {'id': 'msg_probe', 'type': 'message', 'role': 'assistant', 'model': body.get('model', 'claude-sonnet-4-5'), 'content': content, 'stop_reason': stop, 'stop_sequence': None, 'usage': {'input_tokens': 100, 'output_tokens': 20}}
                if not body.get('stream'):
                    self.send_response(200)
                    self.send_header('Content-Type', 'application/json')
                    self.end_headers()
                    self.wfile.write(json.dumps(message).encode())
                    return
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.end_headers()

                def event(name, data):
                    self.wfile.write(('event: ' + name + '\ndata: ' + json.dumps(data) + '\n\n').encode())
                    self.wfile.flush()
                event('message_start', {'type': 'message_start', 'message': {**message, 'content': [], 'stop_reason': None, 'usage': {'input_tokens': 100, 'output_tokens': 0}}})
                c = content[0]
                event('content_block_start', {'type': 'content_block_start', 'index': 0, 'content_block': {**c, **({'input': {}} if c['type'] == 'tool_use' else {'text': ''})}})
                delta = {'type': 'input_json_delta', 'partial_json': json.dumps(c['input'])} if c['type'] == 'tool_use' else {'type': 'text_delta', 'text': c['text']}
                event('content_block_delta', {'type': 'content_block_delta', 'index': 0, 'delta': delta})
                event('content_block_stop', {'type': 'content_block_stop', 'index': 0})
                event('message_delta', {'type': 'message_delta', 'delta': {'stop_reason': stop, 'stop_sequence': None}, 'usage': {'output_tokens': 20}})
                event('message_stop', {'type': 'message_stop'})
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        env = {**os.environ, 'PATH': str(pathlib.Path(args.node).parent) + os.pathsep + os.environ.get('PATH', ''), 'HOME': d, 'CLAUDE_CONFIG_DIR': str(pathlib.Path(d) / 'claude'), 'ANTHROPIC_BASE_URL': f'http://127.0.0.1:{server.server_port}', 'ANTHROPIC_API_KEY': 'local-fixture', 'ANTHROPIC_AUTH_TOKEN': 'local-fixture', 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1', 'DISABLE_AUTOUPDATER': '1'}
        env.pop('CLAUDECODE', None)
        for key in ['CLAUDE_CODE_USE_BEDROCK', 'CLAUDE_CODE_USE_VERTEX', 'CLAUDE_CODE_USE_FOUNDRY']:
            env.pop(key, None)
        proc = subprocess.Popen([args.node, ADAPTER], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env, cwd=d, start_new_session=True)
        messages = queue.Queue()
        threading.Thread(target=lambda : [messages.put(json.loads(l)) for l in proc.stdout if l.strip()], daemon=True).start()
        errors = []
        threading.Thread(target=lambda : [errors.append(l) for l in proc.stderr], daemon=True).start()
        permissions = []
        nxt = 0

        def send(m):
            proc.stdin.write(json.dumps({'jsonrpc': '2.0', **m}) + '\n')
            proc.stdin.flush()

        def call(method, params):
            nonlocal nxt
            nxt += 1
            ident = nxt
            send({'id': ident, 'method': method, 'params': params})
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline:
                if proc.poll() is not None:
                    raise RuntimeError('adapter exited: ' + ''.join(errors)[-1000:])
                try:
                    m = messages.get(timeout=1)
                except queue.Empty:
                    continue
                if m.get('method') == 'session/request_permission':
                    permissions.append(m['params']['toolCall'])
                    send({'id': m['id'], 'result': {'outcome': {'outcome': 'selected', 'optionId': next((o['optionId'] for o in m['params']['options'] if o['kind'] == 'reject_once'))}}})
                elif m.get('id') == ident:
                    if 'error' in m:
                        raise RuntimeError(m['error'])
                    return m.get('result')
            raise TimeoutError('adapter did not finish: ' + ''.join(errors)[-1000:])
        try:
            call('initialize', {'protocolVersion': 1, 'clientCapabilities': {'fs': {'readTextFile': False, 'writeTextFile': False}, 'terminal': False}})
            options = json.loads(args.options_json)
            if not ask:
                options.pop('settings', None)
            created = call('session/new', {'cwd': d, 'mcpServers': [], '_meta': {'claudeCode': {'options': options}}})
            sid = created['sessionId']
            call('session/set_mode', {'sessionId': sid, 'modeId': 'default'})
            call('session/prompt', {'sessionId': sid, 'prompt': [{'type': 'text', 'text': 'Read safe.txt once.'}]})
            assert len(permissions) == int(ask), f'expected {int(ask)} permission requests, got {permissions}'
            assert len(calls) >= 2, 'must actually call the local API and consume a tool result'
            assert tool_results, 'must receive the injected Read result'
            if ask:
                assert permissions[0].get('kind') == 'read' and permissions[0]['rawInput'].get('file_path') == target, 'permission must correspond to the injected file read'
                assert all((r.get('is_error') and marker not in json.dumps(r) for r in tool_results)), 'rejected Read must return an error without file contents'
            else:
                assert any((not r.get('is_error') and marker in json.dumps(r) for r in tool_results)), 'baseline must really read the fixture, not fail before tool execution'
            print(json.dumps({'ask': ask, 'permission_count': len(permissions), 'api_calls': len(calls), 'read_rejected': bool(ask)}), flush=True)
        finally:
            try:
                os.killpg(proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
            server.shutdown()
run(False)
run(True)
