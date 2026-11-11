# Run one shell command on the Nest's USB root shell and print its output.
import os, sys, termios, time, select, tty
fd = os.open('/dev/ttyACM0', os.O_RDWR | os.O_NOCTTY)
tty.setraw(fd)
def read_for(t):
    out = b''; end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.1)
        if r: out += os.read(fd, 65536)
    return out
read_for(0.5)
marker = 'END%d' % os.getpid()
os.write(fd, ('%s; echo %s:$?\r\n' % (sys.argv[1], marker)).encode())
out = b''; end = time.time() + float(sys.argv[2] if len(sys.argv) > 2 else 15)
while time.time() < end:
    r, _, _ = select.select([fd], [], [], 0.2)
    if r: out += os.read(fd, 65536)
    if (marker + ':').encode() in out:
        out += read_for(0.2); break
text = out.decode(errors='replace').replace('\r', '')
print(text.split(marker + ':')[0].rstrip())
