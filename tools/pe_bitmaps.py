# Dump RT_BITMAP (2) and RT_DIALOG (5) resources of a PE32 to a folder.
import sys, struct, os
d = open(sys.argv[1], 'rb').read(); out = sys.argv[2]; os.makedirs(out, exist_ok=True)
pe = struct.unpack_from('<I', d, 0x3c)[0]
nsec = struct.unpack_from('<H', d, pe + 6)[0]; opt = struct.unpack_from('<H', d, pe + 20)[0]
dd = pe + 24 + 96; rsrc_rva = struct.unpack_from('<I', d, dd + 2 * 8)[0]
secs = []
so = pe + 24 + opt
for i in range(nsec):
    va, vs, raw = struct.unpack_from('<I', d, so + 40*i + 12)[0], struct.unpack_from('<I', d, so + 40*i + 8)[0], struct.unpack_from('<I', d, so + 40*i + 20)[0]
    secs.append((va, max(vs, 1 << 30), raw))
def off(rva):
    for va, vs, raw in secs:
        if va <= rva < va + vs and raw: last = rva - va + raw
    return last
base = off(rsrc_rva)
def entries(o):
    nn, ni = struct.unpack_from('<HH', d, base + o + 12)
    for k in range(nn + ni):
        name, ptr = struct.unpack_from('<II', d, base + o + 16 + 8*k)
        if name & 0x80000000:
            p = base + (name & 0x7fffffff); n = struct.unpack_from('<H', d, p)[0]
            name = d[p+2:p+2+2*n].decode('utf-16le')
        yield name, ptr
for t, p1 in entries(0):
    if t not in (2, 5): continue
    for nm, p2 in entries(p1 & 0x7fffffff):
        for lang, p3 in entries(p2 & 0x7fffffff):
            rva, size = struct.unpack_from('<II', d, base + p3)
            data = d[off(rva): off(rva) + size]
            if t == 2:
                hdr = struct.unpack_from('<I', data, 0)[0]
                bpp = struct.unpack_from('<H', data, 14)[0]; clr = struct.unpack_from('<I', data, 32)[0]
                ncol = clr or (1 << bpp if bpp <= 8 else 0)
                bf = b'BM' + struct.pack('<IHHI', 14 + len(data), 0, 0, 14 + hdr + 4*ncol)
                w, h = struct.unpack_from('<ii', data, 4)
                open(f'{out}/bmp_{nm}.bmp', 'wb').write(bf + data); print('BMP', nm, w, h, bpp)
            else:
                open(f'{out}/dlg_{nm}.bin', 'wb').write(data)
