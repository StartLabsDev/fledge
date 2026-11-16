# Receive one TCP connection's bytes into a file.
import socket, sys
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('0.0.0.0', int(sys.argv[1]))); s.listen(1); s.settimeout(30)
c, _ = s.accept(); data = b''
while True:
    b = c.recv(65536)
    if not b: break
    data += b
open(sys.argv[2], 'wb').write(data); print(len(data))
