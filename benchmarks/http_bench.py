#!/usr/bin/env python3
"""
Lightweight HTTP benchmark utility for Chopin.

A wrk-style load generator that works without external dependencies.
Useful for quick smoke-test benchmarks on any platform.

Usage:
    python3 benchmarks/http_bench.py [URL] [--threads N] [--requests N] [--duration S]

Examples:
    python3 benchmarks/http_bench.py
    python3 benchmarks/http_bench.py http://127.0.0.1:8080/plaintext
    python3 benchmarks/http_bench.py http://127.0.0.1:8080/json --threads 8 --duration 30
    python3 benchmarks/http_bench.py http://127.0.0.1:8080/plaintext --requests 100000
"""
import argparse
import socket
import sys
import time
import threading
from urllib.parse import urlparse


REQUEST_TEMPLATE = (
    "GET {path} HTTP/1.1\r\n"
    "Host: {host}\r\n"
    "Connection: keep-alive\r\n"
    "User-Agent: chopin-bench/1.0\r\n"
    "\r\n"
).encode()

RESPONSE_HEADER_END = b"\r\n\r\n"


def parse_response(data: bytes) -> tuple[int, int]:
    """Return (status_code, body_length) from raw HTTP response bytes."""
    header_end = data.find(RESPONSE_HEADER_END)
    if header_end == -1:
        return 0, 0
    header = data[:header_end].decode("latin-1")
    status_line = header.split("\r\n", 1)[0]
    try:
        status = int(status_line.split(" ", 2)[1])
    except (IndexError, ValueError):
        status = 0
    body_len = len(data) - header_end - 4
    return status, body_len


def worker_thread(
    host: str,
    port: int,
    path: str,
    stop_event: threading.Event,
    results: dict,
    lock: threading.Lock,
    request_limit: int,
) -> None:
    request = REQUEST_TEMPLATE.replace(b"{path}", path.encode()).replace(
        b"{host}", f"{host}:{port}".encode()
    )
    local_ok = 0
    local_err = 0
    local_times: list[float] = []
    count = 0

    try:
        sock = socket.create_connection((host, port), timeout=5)
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    except OSError as e:
        with lock:
            results["errors"] += 1
        return

    try:
        while not stop_event.is_set():
            if request_limit > 0 and count >= request_limit:
                break
            try:
                t0 = time.perf_counter()
                sock.sendall(request)
                data = b""
                while RESPONSE_HEADER_END not in data:
                    chunk = sock.recv(4096)
                    if not chunk:
                        raise ConnectionResetError("connection closed")
                    data += chunk
                    if len(data) > 65536:
                        break
                elapsed_ms = (time.perf_counter() - t0) * 1000
                status, _ = parse_response(data)
                if 200 <= status < 300:
                    local_ok += 1
                    local_times.append(elapsed_ms)
                else:
                    local_err += 1
                count += 1
            except OSError:
                local_err += 1
                # Reconnect on error
                try:
                    sock.close()
                    sock = socket.create_connection((host, port), timeout=5)
                    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
                except OSError:
                    break
    finally:
        sock.close()

    with lock:
        results["ok"] += local_ok
        results["errors"] += local_err
        results["times"].extend(local_times)


def benchmark(url: str, num_threads: int, duration: float, request_limit: int) -> None:
    parsed = urlparse(url)
    host = parsed.hostname or "127.0.0.1"
    port = parsed.port or 80
    path = parsed.path or "/"
    if parsed.query:
        path += "?" + parsed.query

    # Warmup check
    try:
        sock = socket.create_connection((host, port), timeout=3)
        sock.close()
    except OSError as e:
        print(f"Error: cannot connect to {host}:{port} — {e}", file=sys.stderr)
        sys.exit(1)

    results = {"ok": 0, "errors": 0, "times": []}
    lock = threading.Lock()
    stop_event = threading.Event()

    per_thread_limit = (request_limit // num_threads) if request_limit > 0 else 0

    threads = [
        threading.Thread(
            target=worker_thread,
            args=(host, port, path, stop_event, results, lock, per_thread_limit),
            daemon=True,
        )
        for _ in range(num_threads)
    ]

    print(f"Benchmarking {url}")
    print(f"Threads: {num_threads}  ", end="")
    if duration > 0:
        print(f"Duration: {duration:.0f}s")
    else:
        print(f"Requests: {request_limit}")
    print("")

    t_start = time.perf_counter()
    for t in threads:
        t.start()

    if duration > 0:
        time.sleep(duration)
        stop_event.set()
    for t in threads:
        t.join(timeout=duration + 10 if duration > 0 else 60)
    t_total = time.perf_counter() - t_start

    # Stats
    times = sorted(results["times"])
    total_req = results["ok"] + results["errors"]
    rps = results["ok"] / t_total if t_total > 0 else 0

    print(f"{'='*50}")
    print(f"Requests completed:  {results['ok']:,}")
    print(f"Errors:              {results['errors']:,}")
    print(f"Duration:            {t_total:.2f}s")
    print(f"Throughput:          {rps:,.0f} req/s")
    if times:
        avg = sum(times) / len(times)
        p50 = times[int(len(times) * 0.50)]
        p75 = times[int(len(times) * 0.75)]
        p90 = times[int(len(times) * 0.90)]
        p99 = times[int(len(times) * 0.99)]
        print(f"")
        print(f"Latency (ms):")
        print(f"  Min:  {times[0]:.3f}")
        print(f"  Avg:  {avg:.3f}")
        print(f"  P50:  {p50:.3f}")
        print(f"  P75:  {p75:.3f}")
        print(f"  P90:  {p90:.3f}")
        print(f"  P99:  {p99:.3f}")
        print(f"  Max:  {times[-1]:.3f}")
    print(f"{'='*50}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Chopin HTTP benchmark tool (no external dependencies)"
    )
    parser.add_argument(
        "url",
        nargs="?",
        default="http://127.0.0.1:8080/plaintext",
        help="URL to benchmark (default: http://127.0.0.1:8080/plaintext)",
    )
    parser.add_argument(
        "--threads", "-t", type=int, default=8, help="Number of threads (default: 8)"
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--duration", "-d", type=float, default=10.0, help="Test duration in seconds (default: 10)"
    )
    group.add_argument(
        "--requests", "-n", type=int, default=0, help="Total requests to send (overrides --duration)"
    )
    args = parser.parse_args()

    benchmark(args.url, args.threads, args.duration if args.requests == 0 else 0, args.requests)


if __name__ == "__main__":
    main()
