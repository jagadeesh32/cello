import requests
import time
import subprocess
import sys

def run_test():
    server = subprocess.Popen(
        [sys.executable, "examples/circuit_breaker.py"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )
    
    print("🚀 Circuit Breaker Demo running at http://127.0.0.1:8082")
    base_url = "http://127.0.0.1:8082"
    
    try:
        time.sleep(3)
        
        # 1. Closed state check
        print("Testing Closed State...")
        resp = requests.get(f"{base_url}/")
        assert resp.status_code == 200
        print("✅ Closed state verified (200 OK)")
        
        # 2. Trigger failures
        # Config: 3 failures to open
        print("Triggering failures...")
        for i in range(3):
            # fail=true triggers 500
            resp = requests.get(f"{base_url}/test_cb?fail=true")
            print(f"Call {i+1}: Status {resp.status_code}")
            assert resp.status_code == 500
            
        # 3. Verify Open State
        print("Testing Open State (should be 503)...")
        # Same endpoint, no failure param, but should be blocked
        resp = requests.get(f"{base_url}/test_cb")
        print(f"Status: {resp.status_code}")
        assert resp.status_code == 503
        print("✅ Open state verified")
        
        # 4. Wait for Half-Open
        # Timeout is 5s
        print("Waiting 5.5s for Half-Open...")
        time.sleep(5.5)
        
        # 5. Verify successful recovery
        # Half-open target is 1
        print("Triggering recovery...")
        # Successful request acts as probe
        resp = requests.get(f"{base_url}/test_cb")
        print(f"Recovery status: {resp.status_code}")
        assert resp.status_code == 200
        # Should now be Closed
        
        # 6. Verify Closed again
        print("Testing Closed State after recovery...")
        resp = requests.get(f"{base_url}/test_cb")
        assert resp.status_code == 200
        print("✅ Recovery verified")
        
        print("Circuit Breaker Test Passed")

    except Exception as e:
        print(f"Test failed: {e}")
        # server.terminate() # moved to finally
        # print server stderr
        out, err = server.communicate(timeout=1)
        print("Server Stderr:", err)
        raise e
    finally:
        server.terminate()
        server.wait()

if __name__ == "__main__":
    run_test()
