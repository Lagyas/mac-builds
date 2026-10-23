# Minimal WinHelp (.hlp) text extractor: |TOPIC LZ77 blocks -> SJIS text runs.
import sys, struct, re
sys.stdout.reconfigure(encoding="utf-8")
def files(d):
    out = {}
    for m in re.finditer(rb'\|([A-Za-z]+)\x00', d):
        out[m.group(1).decode()] = struct.unpack_from('<i', d, m.end())[0]
    return out
def lz77(src):
    out = bytearray(); i = 0
    while i < len(src):
        flags = src[i]; i += 1
        for b in range(8):
            if i >= len(src): break
            if flags & (1 << b):
                t = struct.unpack_from('<H', src, i)[0]; i += 2
                pos = len(out) - ((t & 0xFFF) + 1); n = (t >> 12) + 3
                for k in range(n): out.append(out[pos + k])
            else:
                out.append(src[i]); i += 1
    return bytes(out)
d = open(sys.argv[1], 'rb').read()
f = files(d)
so = f['SYSTEM']; sysd = d[so + 9:]
minor, flags = struct.unpack_from('<H', sysd, 2)[0], struct.unpack_from('<H', sysd, 10)[0]
bs = 2048 if (minor <= 16 or flags == 8) else 4096
comp = minor > 16 and flags in (4, 8)
to = f['TOPIC']; used = struct.unpack_from('<i', d, to + 4)[0]
t = d[to + 9: to + 9 + used]
buf = b''
for off in range(0, len(t), bs):
    blk = t[off + 12: off + bs]
    buf += lz77(blk) if comp else blk
txt = buf.decode('cp932', errors='replace')
for run in re.findall(r'[^\x00-\x1f�]{2,}', txt):
    print(run)
