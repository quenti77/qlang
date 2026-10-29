#!/usr/bin/env python3
"""HTTP load test for `qlang serve` (requests per second, latency, memory over time).

    python3 benches/http_load.py [--binary target/release/qlang] [--workers 4]
        [--connections 32] [--duration 10] [--scenario hello,json,compute,echo] [--json out.json]

It starts the server on benches/http/app.q, warms it up, then for each scenario keeps
`--connections` keep-alive connections busy for `--duration` seconds from a few client
processes. It prints requests/s, the p50/p95/p99/max latency in ms, the errors, and the
server's resident memory before and after (read from /proc, Linux only). The server RSS is
sampled every second, so a leak shows as a line that keeps climbing: the last column is the
growth per minute over the second half of the run.
"""
import argparse
import asyncio
import json
import multiprocessing
import os
import subprocess
import sys
import tempfile
import time

SCENARIOS = {
    "hello": ("GET", "/hello", b""),
    "json": ("GET", "/json", b""),
    "compute": ("GET", "/compute", b""),
    "echo": ("POST", "/echo", b"x" * 512),
}


def request_bytes(method, path, body):
    head = f"{method} {path} HTTP/1.1\r\nHost: bench\r\n"
    if body:
        head += f"Content-Length: {len(body)}\r\n"
    return head.encode() + b"\r\n" + body


async def one_connection(port, raw, deadline, latencies, errors):
    reader = writer = None
    served = 0
    while time.perf_counter() < deadline:
        try:
            # the server closes a connection after 100 requests: open another one
            if writer is None or served >= 90:
                if writer is not None:
                    writer.close()
                reader, writer = await asyncio.open_connection("127.0.0.1", port)
                served = 0
            started = time.perf_counter()
            writer.write(raw)
            head = await reader.readuntil(b"\r\n\r\n")
            length = 0
            for line in head.split(b"\r\n"):
                if line.lower().startswith(b"content-length:"):
                    length = int(line.split(b":")[1])
            if length:
                await reader.readexactly(length)
            if not head.startswith(b"HTTP/1.1 200"):
                errors.append(head.split(b"\r\n")[0].decode())
            latencies.append(time.perf_counter() - started)
            served += 1
        except (OSError, asyncio.IncompleteReadError):
            errors.append("connection")
            writer = None
            await asyncio.sleep(0.01)
    if writer is not None:
        writer.close()


def client_process(port, raw, connections, duration, out):
    latencies, errors = [], []
    deadline = time.perf_counter() + duration

    async def main():
        await asyncio.gather(
            *[one_connection(port, raw, deadline, latencies, errors) for _ in range(connections)]
        )

    asyncio.run(main())
    out.put((latencies, errors[:20], len(errors)))


def rss_kb(pid):
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
    except OSError:
        pass
    return 0


def percentile(sorted_values, p):
    if not sorted_values:
        return 0.0
    return sorted_values[min(len(sorted_values) - 1, int(len(sorted_values) * p))]


def run_scenario(name, port, pid, connections, duration, clients):
    method, path, body = SCENARIOS[name]
    raw = request_bytes(method, path, body)
    ctx = multiprocessing.get_context("fork")
    out = ctx.Queue()
    deadline = time.perf_counter() + duration
    per = [connections // clients + (1 if i < connections % clients else 0) for i in range(clients)]
    procs = [
        ctx.Process(target=client_process, args=(port, raw, n, duration, out))
        for n in per
        if n > 0
    ]
    before = rss_kb(pid)
    for p in procs:
        p.start()
    samples = []
    while time.perf_counter() < deadline:
        time.sleep(1)
        samples.append((time.perf_counter(), rss_kb(pid)))
    latencies, errors, error_count = [], [], 0
    for _ in procs:
        l, e, c = out.get()
        latencies += l
        errors += e
        error_count += c
    for p in procs:
        p.join()
    latencies.sort()
    after = rss_kb(pid)
    half = samples[len(samples) // 2 :]
    growth = 0.0
    if len(half) >= 2 and half[-1][0] > half[0][0]:
        growth = (half[-1][1] - half[0][1]) / (half[-1][0] - half[0][0]) * 60 / 1024
    return {
        "scenario": name,
        "requests": len(latencies),
        "rps": len(latencies) / duration,
        "p50_ms": percentile(latencies, 0.50) * 1000,
        "p95_ms": percentile(latencies, 0.95) * 1000,
        "p99_ms": percentile(latencies, 0.99) * 1000,
        "max_ms": (latencies[-1] if latencies else 0) * 1000,
        "errors": error_count,
        "error_samples": sorted(set(errors)),
        "rss_before_mb": before / 1024,
        "rss_after_mb": after / 1024,
        "rss_growth_mb_per_min": growth,
    }


def start_server(binary, workers, port):
    # the server's log goes to a file: a pipe nobody reads would fill up and stop the server
    log = tempfile.NamedTemporaryFile(prefix="qlang-bench-", suffix=".log", delete=False)
    server = subprocess.Popen(
        [binary, "serve", "benches/http/app.q", "--port", str(port), "--workers", str(workers),
         "--no-access-log"],
        stderr=log,
    )
    for _ in range(100):
        time.sleep(0.1)
        with open(log.name) as f:
            started = "serving" in f.read()
        if started:
            server.log_path = log.name
            return server
        if server.poll() is not None:
            break
    server.kill()
    with open(log.name) as f:
        reason = f.read().strip()
    os.unlink(log.name)
    sys.exit(f"the server did not start: {reason}")


def free_port():
    import socket

    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    parser.add_argument("--binary", default="./target/release/qlang")
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--connections", type=int, default=32)
    parser.add_argument("--duration", type=int, default=10)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--clients", type=int, default=min(4, os.cpu_count() or 1))
    parser.add_argument("--scenario", default=",".join(SCENARIOS))
    parser.add_argument("--json", help="also write the results to this file")
    args = parser.parse_args()
    names = args.scenario.split(",")
    for n in names:
        if n not in SCENARIOS:
            sys.exit(f"unknown scenario `{n}` (one of {', '.join(SCENARIOS)})")

    port = free_port()
    server = start_server(args.binary, args.workers, port)
    results = []
    try:
        if args.warmup:
            run_scenario("hello", port, server.pid, args.connections, args.warmup, args.clients)
        for name in names:
            results.append(
                run_scenario(name, port, server.pid, args.connections, args.duration, args.clients)
            )
    finally:
        server.terminate()
        server.wait()
        os.unlink(server.log_path)

    print(
        f"{args.workers} worker(s), {args.connections} connection(s), {args.duration} s, "
        f"{os.cpu_count()} cores\n"
    )
    print(f"{'scenario':9} {'req/s':>9} {'p50 ms':>8} {'p95 ms':>8} {'p99 ms':>8} {'max ms':>8} "
          f"{'errors':>6} {'RSS MB':>14} {'MB/min':>7}")
    for r in results:
        print(
            f"{r['scenario']:9} {r['rps']:9.0f} {r['p50_ms']:8.2f} {r['p95_ms']:8.2f} "
            f"{r['p99_ms']:8.2f} {r['max_ms']:8.2f} {r['errors']:6d} "
            f"{r['rss_before_mb']:6.0f} -> {r['rss_after_mb']:4.0f} {r['rss_growth_mb_per_min']:7.1f}"
        )
        if r["error_samples"]:
            print("   errors:", ", ".join(r["error_samples"]))
    if args.json:
        with open(args.json, "w") as f:
            json.dump({"workers": args.workers, "connections": args.connections,
                       "duration": args.duration, "results": results}, f, indent=2)
    return 1 if any(r["errors"] for r in results) else 0


if __name__ == "__main__":
    sys.exit(main())
