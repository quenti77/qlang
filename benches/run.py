import subprocess, sys, time, glob, os
binary = sys.argv[1] if len(sys.argv) > 1 else "./target/release/qlang"
for f in sorted(glob.glob("benches/*.q")):
    best = 1e9
    for _ in range(5):
        t = time.time()
        r = subprocess.run([binary, "run", "--max-steps", "100000000000", f], capture_output=True, text=True)
        best = min(best, time.time() - t)
        if r.returncode != 0:
            print(f, "FAILED", r.stderr[:200]); break
    print(f"{os.path.basename(f):12} {best:6.2f}s")
