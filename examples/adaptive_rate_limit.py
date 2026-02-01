from cello import App, RateLimitConfig, Response
import time
import threading

app = App()

# Enable adaptive rate limit
# Base: 100 req/min
# Adaptive: Throttle to 5 req/min if error rate > 20%
config = RateLimitConfig.adaptive(
    capacity=100,
    refill_rate=100,
    min_capacity=5,
    error_threshold=0.20
)
app.enable_rate_limit(config)

@app.get("/")
def home(request):
    return {"status": "ok", "message": "System is healthy"}

@app.get("/trigger-errors")
def trigger_errors(request):
    """Endpoint that returns errors to trigger adaptive limiting"""
    return Response.json({"error": "Simulated Warning"}, status=500)

if __name__ == "__main__":
    print("🚀 Adaptive Rate Limit Demo running at http://127.0.0.1:8080")
    print("Test scenario:")
    print("1. Send requests to / -> Should be fast")
    print("2. Send many requests to /trigger-errors -> Limit should drop")
    print("3. Excess requests to / should be 429'd")
    app.run(port=8080)
