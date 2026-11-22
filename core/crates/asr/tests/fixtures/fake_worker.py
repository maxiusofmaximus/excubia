"""Stand-in for asr_worker.py. The mode comes in as the model argument; it picks how the worker fails."""

import json
import sys
import time

mode = sys.argv[1]
requests = sys.stdin.buffer
protocol = sys.stdout.buffer


def send(message):
    protocol.write((json.dumps(message) + "\n").encode("ascii"))
    protocol.flush()


if mode == "no_cuda":
    send({"error": "no_cuda"})
    sys.exit(1)
send({"ready": True, "provider": "CUDAExecutionProvider"})

while True:
    line = requests.readline()
    if not line:
        break
    header = json.loads(line)
    requests.read(header["samples"] * 4)
    if mode == "die":
        sys.exit(1)
    if mode == "hang":
        time.sleep(3600)
    if mode == "garbage":
        protocol.write(b"not json\n")
        protocol.flush()
    elif mode == "error":
        send({"id": header["id"], "error": "inference_failed"})
    else:
        send({"id": header["id"], "text": f"muestras {header['samples']}", "ms": 1})
