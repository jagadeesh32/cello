import requests
import time
import subprocess
import sys
import os

def run_test():
    # Start the example server
    server = subprocess.Popen(
        [sys.executable, "examples/dto_validation.py"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )
    
    print("🚀 DTO Validation Demo running at http://127.0.0.1:8080")
    base_url = "http://127.0.0.1:8080"
    
    try:
        # Wait for server to start
        time.sleep(5)
        
        # 1. Valid Request
        print("Testing Valid Request...")
        payload = {
            "username": "jdoe",
            "email": "jdoe@example.com",
            "age": 30
        }
        resp = requests.post(f"{base_url}/users", json=payload)
        assert resp.status_code == 200
        data = resp.json()
        assert data["user"]["username"] == "jdoe"
        print("✅ Valid request verified")
        
        # 2. Invalid Request (Missing Field)
        print("Testing Invalid Request (Missing Field)...")
        payload = {
            "username": "jdoe",
            # email missing
            "age": 30
        }
        resp = requests.post(f"{base_url}/users", json=payload)
        print(f"<-- POST /users {resp.status_code} {resp.text}")
        assert resp.status_code == 422
        assert "validation error" in resp.text.lower() or "missing" in resp.text.lower()
        print("✅ Missing field validation verified")
        
        # 3. Invalid Request (Type Mismatch)
        print("Testing Invalid Request (Type Mismatch)...")
        payload = {
            "username": "jdoe",
            "email": "jdoe@example.com",
            "age": "invalid_age" # Should be int
        }
        resp = requests.post(f"{base_url}/users", json=payload)
        print(f"<-- POST /users {resp.status_code} {resp.text}")
        assert resp.status_code == 422
        print("✅ Type validation verified")
        
        print("DTO Validation Test Passed")
        
    except Exception as e:
        print(f"Test failed: {e}")
        error = server.stderr.read()
        print(f"Server stderr: {error}")
        raise e
    finally:
        server.terminate()
        server.wait()

if __name__ == "__main__":
    run_test()
