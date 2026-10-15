#!/usr/bin/env python3
# fake greetd for t-greeter: password "pw123"; logs every request to fake-greetd.log
import json, os, socket, struct, sys
path = sys.argv[1]
if os.path.exists(path): os.unlink(path)
srv = socket.socket(socket.AF_UNIX); srv.bind(path); srv.listen(1)
log = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "fake-greetd.log"), "w")
def recv(c):
    h = c.recv(4, socket.MSG_WAITALL)
    if len(h) < 4: return None
    return json.loads(c.recv(struct.unpack("=I", h)[0], socket.MSG_WAITALL))
def send(c, v):
    b = json.dumps(v).encode(); c.sendall(struct.pack("=I", len(b)) + b)
while True:
    c, _ = srv.accept(); ok = False
    while (r := recv(c)) is not None:
        log.write(json.dumps({k: ("***" if k == "response" else v) for k, v in r.items()}) + "\n"); log.flush()
        t = r["type"]
        if t == "create_session": send(c, {"type": "auth_message", "auth_message_type": "secret", "auth_message": "Password: "})
        elif t == "post_auth_message_response":
            ok = r["response"] == "pw123"
            send(c, {"type": "success"} if ok else {"type": "error", "error_type": "auth_error", "description": "pam"})
        elif t == "start_session": send(c, {"type": "success"} if ok else {"type": "error", "error_type": "error", "description": "no"})
        else: send(c, {"type": "success"})
    c.close()
