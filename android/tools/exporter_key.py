# The key horizon-prepare gives an exporter bundle (crates/horizon-prepare/src/exporters.rs,
# `key`): its compiler settings, its own source and the sources it compiles.
import hashlib, os, sys
root = sys.argv[1]
PARITY = 'mods/horizon/src/test/java/dev/horizonlab/parity'
def files(d):
    out = []
    for e in os.listdir(d):
        p = os.path.join(d, e)
        out.extend(files(p) if os.path.isdir(p) else [p])
    out.sort()
    return out
h = hashlib.sha256()
h.update(b"exporters-v1 --release -proc:none -encoding UTF-8")
h.update(open(os.path.join(root, 'crates/horizon-prepare/src/exporters.rs'), encoding='utf-8').read().replace('\r', '').encode())
src = [p for p in files(os.path.join(root, PARITY)) if p.endswith('.java')]
src.append(os.path.join(root, 'research-lock.json'))
for p in src:
    data = open(p, 'rb').read().replace(b'\r', b'')
    rel = os.path.relpath(p, root).replace('\\', '/')
    h.update(f"{rel}\0{len(data)}\0".encode())
    h.update(data)
print(h.hexdigest())
