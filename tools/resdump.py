"""Dump PE resources: bitmaps -> .png, others -> .bin.  usage: resdump.py <dll> <outdir>"""
import sys, os, struct, io, pefile
from PIL import Image
dll, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
pe = pefile.PE(dll)
for t in pe.DIRECTORY_ENTRY_RESOURCE.entries:
    tname = str(t.name) if t.name else str(t.id)
    for n in t.directory.entries:
        l = n.directory.entries[0]
        d = pe.get_data(l.data.struct.OffsetToData, l.data.struct.Size)
        nid = str(n.name) if n.name else str(n.id)
        base = os.path.join(out, "%s_%s" % (tname, nid))
        if t.id == 2:
            hdr = struct.unpack_from("<I", d)[0]; bpp = struct.unpack_from("<H", d, 14)[0]
            ncol = struct.unpack_from("<I", d, 32)[0] or (1 << bpp if bpp <= 8 else 0)
            fh = b"BM" + struct.pack("<IHHI", 14 + len(d), 0, 0, 14 + hdr + ncol * 4)
            try:
                im = Image.open(io.BytesIO(fh + d)); im.save(base + ".png"); print(base, im.size, bpp)
            except Exception as e:
                print(base, "err", e)
        else:
            open(base + ".bin", "wb").write(d)
