import requests
import time
import subprocess
import sys

server_process = subprocess.Popen([sys.executable, "examples/smart_caching.py"])
time.sleep(2)

try:
    print("Testing Smart Caching...")
    base_url = "http://127.0.0.1:8080"
    
    # 1. Test TTL Override
    print("Testing /cached (10s TTL)...")
    resp1 = requests.get(f"{base_url}/cached").json()
    time.sleep(1)
    resp2 = requests.get(f"{base_url}/cached").json()
    
    assert resp1["timestamp"] == resp2["timestamp"], "Timestamp should match (cached)"
    print("✅ Cached response verified")
    
    # 2. Test Tagged Caching
    print("Testing /tagged (Tag: 'users')...")
    resp_tag1 = requests.get(f"{base_url}/tagged").json()
    time.sleep(1)
    resp_tag2 = requests.get(f"{base_url}/tagged").json()
    
    assert resp_tag1["timestamp"] == resp_tag2["timestamp"], "Timestamp should match (tagged)"
    print("✅ Tagged response verified")
    
    # 3. Test Invalidation
    print("Invalidating 'users' tag...")
    inv_resp = requests.post(f"{base_url}/invalidate")
    assert inv_resp.status_code == 200
    
    time.sleep(0.5) # Allow async invalidation to complete
    
    resp_tag3 = requests.get(f"{base_url}/tagged").json()
    assert resp_tag3["timestamp"] != resp_tag1["timestamp"], "Timestamp should change after invalidation"
    print("✅ Cache invalidation verified")

    print("Smart Caching Test Passed")

finally:
    server_process.terminate()
    server_process.wait()
