from cello import App, Response
import time

app = App()

# Config: 
# - Open after 3 failures
# - Wait 5 seconds to try again (Half-Open)
# - Need 1 success in Half-Open to Close
app.enable_circuit_breaker(failure_threshold=3, reset_timeout=5, half_open_target=1)

@app.get("/")
def home(request):
    return {"status": "ok", "message": "System is healthy"}

@app.get("/flaky")
def flaky(request):
    """Always returns 500 to trigger CB"""
    print("Flaky endpoint called")
    return Response.error(500, "Simulated Failure")

@app.get("/recover")
def recover(request):
    """Returns 200 to help close CB"""
    print("Recover endpoint called")
    return {"status": "recovered"}

@app.get("/test_cb")
def test_cb(request):
    """Controllable endpoint for CB testing"""
    if "fail" in request.query_params and request.query_params["fail"] == "true":
        return Response.error(500, "Simulated Failure")
    return {"status": "ok"}

if __name__ == "__main__":
    print("🚀 Circuit Breaker Demo at http://127.0.0.1:8082")
    print("1. Hit /flaky 3 times -> Circuit Opens (503 Service Unavailable)")
    print("2. Hit / (or any endpoint) -> Returns 503 while Open")
    print("3. Wait 5 seconds...")
    print("4. Hit /recover -> Circuit enters Half-Open, then Closed")
    app.run(port=8082)
