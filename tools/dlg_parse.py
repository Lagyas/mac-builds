# Parse a classic DLGTEMPLATE (non-EX) resource dump.
import sys, struct
d = open(sys.argv[1], 'rb').read(); p = 0
def u16():
    global p; v = struct.unpack_from('<H', d, p)[0]; p += 2; return v
def sz_or_ord():
    global p
    if struct.unpack_from('<H', d, p)[0] == 0xFFFF: p += 2; return '#%d' % u16()
    s = ''
    while True:
        c = u16()
        if c == 0: return s
        s += chr(c)
style, ex, n, x, y, cx, cy = struct.unpack_from('<IIHhhhh', d, 0); p = 18
menu = sz_or_ord(); cls = sz_or_ord(); title = sz_or_ord()
if style & 0x40: u16(); font = sz_or_ord()
print('DIALOG', title, x, y, cx, cy, 'items', n)
CL = {0x80: 'Button', 0x81: 'Edit', 0x82: 'Static', 0x83: 'ListBox', 0x84: 'ScrollBar', 0x85: 'ComboBox'}
for i in range(n):
    p = (p + 3) & ~3
    st, ex, x, y, cx, cy, idn = struct.unpack_from('<IIhhhhH', d, p); p += 18
    c = sz_or_ord(); c = CL.get(int(c[1:]), c) if c.startswith('#') else c
    t = sz_or_ord(); extra = u16(); p += extra
    print(f'{c:16} id={idn:5} {x:4} {y:4} {cx:4} {cy:4} style={st:08x} {t!r}')
