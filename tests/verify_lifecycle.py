import subprocess
import sys
import time
import requests
import os

def run_test():
    # Start the example server
    print("Starting examples/lifecycle_hooks.py...")
    server = subprocess.Popen(
        [sys.executable, "examples/lifecycle_hooks.py"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=0 # Unbuffered to see output immediately? No, read from pipe later
    )
    
    try:
        # Wait for server to start (should trigger startup hook)
        time.sleep(3)
        
        # Check if server is running by hitting health endpoint (if any, or just root)
        # The example has /
        try:
            resp = requests.get("http://127.0.0.1:8080/")
            assert resp.status_code == 200
            print("✅ Server responded (Startup successful)")
        except Exception as e:
            print(f"❌ Server check failed: {e}")
            raise e
            
        # Stop the server (should trigger shutdown hook)
        print("Stopping server...")
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
            
        print("Server stopped.")
        
        # Check output for hooks
        stdout, stderr = server.communicate()
        output = stdout + stderr
        
        print("--- Server Output ---")
        print(output)
        print("---------------------")
        
        if "Startup hook executed!" in output:
            print("✅ Startup hook verified")
        else:
            print("❌ Startup hook NOT found")
            
        if "Shutdown hook executed!" in output:
             print("✅ Shutdown hook verified")
        else:
             print("❌ Shutdown hook NOT found")
             
        if "Startup hook executed!" in output and "Shutdown hook executed!" in output:
            print("Lifecycle Hooks Test Passed")
        else:
            raise Exception("Lifecycle hooks missing from output")

    except Exception as e:
        server.terminate()
        stdout, stderr = server.communicate()
        print("--- Server Output (Error) ---")
        print(stdout + stderr)
        raise e

if __name__ == "__main__":
    run_test()
