#!/usr/bin/env python3
"""Simple HTTP benchmarking tool as alternative to wrk."""
import sys
import time
import threading
from urllib.request import urlopen
from urllib.error import URLError

def benchmark(url, num_threads=16, num_requests=1000):
    """Run a simple benchmark against the given URL."""
    results = {"success": 0, "failed": 0, "times": []}
    lock = threading.Lock()
    
    def worker():
        local_results = {"success": 0, "failed": 0, "times": []}
        requests_per_thread = num_requests // num_threads
        for _ in range(requests_per_thread):
            try:
                start = time.time()
                with urlopen(url, timeout=5) as response:
                    response.read()
                elapsed = (time.time() - start) * 1000  # ms
                local_results["times"].append(elapsed)
                local_results["success"] += 1
            except (URLError, Exception) as e:
                print(f"Error: {e}")
                local_results["failed"] += 1
        
        with lock:
            results["success"] += local_results["success"]
            results["failed"] += local_results["failed"]
            results["times"].extend(local_results["times"])
    
    print(f"Benchmarking {url}")
    print(f"Threads: {num_threads}, Requests: {num_requests}")
    print("Running...")
    
    start_time = time.time()
    threads = []
    for _ in range(num_threads):
        t = threading.Thread(target=worker)
        t.start()
        threads.append(t)
    
    for t in threads:
        t.join()
    
    elapsed = time.time() - start_time
    
    # Stats
    times = sorted(results["times"])
    print(f"\n=== Results ===")
    print(f"Total time: {elapsed:.2f}s")
    print(f"Successful requests: {results['success']}")
    print(f"Failed requests: {results['failed']}")
    print(f"Requests/sec: {results['success'] / elapsed:.2f}")
    if times:
        print(f"Min: {times[0]:.2f}ms")
        print(f"Avg: {sum(times) / len(times):.2f}ms")
        print(f"Max: {times[-1]:.2f}ms")
        print(f"P50: {times[len(times)//2]:.2f}ms")
        print(f"P90: {times[int(len(times)*0.9)]:.2f}ms")
        print(f"P99: {times[int(len(times)*0.99)]:.2f}ms")

if __name__ == "__main__":
    url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8000/plaintext"
    threads = int(sys.argv[2]) if len(sys.argv) > 2 else 16
    requests = int(sys.argv[3]) if len(sys.argv) > 3 else 10000
    benchmark(url, threads, requests)
