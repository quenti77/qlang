import subprocess, sys, time, glob, os
a, b = sys.argv[1], sys.argv[2]
def best(binary, f):
    out = 1e9
    for _ in range(5):
        t = time.time()
        r = subprocess.run([binary, "run", "--max-steps", "100000000000", f], capture_output=True, text=True)
        out = min(out, time.time() - t)
        assert r.returncode == 0, r.stderr[:200]
    return out
print(f"{'':12} {'base':>7} {'new':>7}  speedup")
for f in sorted(glob.glob("benches/*.q")):
    x, y = best(a, f), best(b, f)
    print(f"{os.path.basename(f):12} {x:6.2f}s {y:6.2f}s  {x / y:5.2f}x")
