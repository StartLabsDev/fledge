# Serve one file to the first TCP connection.
import socket, sys
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('0.0.0.0', int(sys.argv[1]))); s.listen(1); s.settimeout(60)
c, _ = s.accept(); c.sendall(open(sys.argv[2], 'rb').read()); c.shutdown(socket.SHUT_WR); c.settimeout(30)
while c.recv(4096): pass
c.close(); print('sent')
