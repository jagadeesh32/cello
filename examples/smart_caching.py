from cello import App, cache, Response
import time

app = App()

# Enable caching with 60s default TTL
app.enable_caching(ttl=60)

@app.get("/")
def home(request):
    return {"message": "Hello! Try /cached or /slow"}

@app.get("/cached")
@cache(ttl=10) # Override TTL to 10s
def cached_endpoint(request):
    # This will change only every 10 seconds
    return {"timestamp": time.time(), "note": "Refreshes every 10s"}

@app.get("/tagged")
@cache(tags=["users", "list"])
def tagged_endpoint(request):
    return {"data": "User List", "timestamp": time.time()}

@app.post("/invalidate")
def invalidate(request):
    # Invalidate "users" tag
    app.invalidate_cache(["users"])
    return {"status": "Cache invalidated for tag 'users'"}

if __name__ == "__main__":
    print("🚀 Smart Caching Demo running at http://127.0.0.1:8080")
    print("Test scenario:")
    print("1. /cached -> timestamp freezes for 10s")
    print("2. /tagged -> timestamp freezes for 60s (default)")
    print("3. POST /invalidate -> /tagged timestamp updates on next call")
    app.run(port=8080)
