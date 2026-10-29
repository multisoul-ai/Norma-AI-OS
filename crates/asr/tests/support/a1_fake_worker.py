"""Binary worker fixture: never imports inference libraries."""
import json
import os
import struct
import sys
import time

mode = sys.argv[1]

def send(value):
    data = json.dumps(value).encode()
    sys.stdout.buffer.write(struct.pack('<I', len(data)) + data)
    sys.stdout.buffer.flush()

send(dict(v=1, op='ready', source='qwen'))
while prefix := sys.stdin.buffer.read(4):
    request = json.loads(sys.stdin.buffer.read(struct.unpack('<I', prefix)[0]))
    sys.stdin.buffer.read(request['bytes'])
    if mode == 'hang':
        time.sleep(30)
    if mode == 'oversized':
        sys.stdout.buffer.write(struct.pack('<I', 65537))
        sys.stdout.buffer.flush()
        continue
    response = dict(request, text=str(os.getpid()), source='qwen', fallback=False)
    if mode == 'stale':
        response['session'] += 1
    if mode == 'fallback':
        response['fallback'] = True
    send(response)
