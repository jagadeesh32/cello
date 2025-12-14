"""Test async and sync handlers work correctly."""
import asyncio
from cello import App

app = App()

# Sync handler
@app.get("/sync")
def sync_handler(request):
    return {"type": "sync", "message": "Hello from sync handler!"}

# Async handler with simulated async operation
@app.get("/async")
async def async_handler(request):
    # Simulate async database call
    await asyncio.sleep(0.1)
    return {"type": "async", "message": "Hello from async handler!"}

# Async handler that uses request data
@app.get("/users/{id}")
async def get_user(request):
    user_id = request.params["id"]
    # Simulate async database lookup
    await asyncio.sleep(0.05)
    return {"id": user_id, "name": f"User {user_id}"}

if __name__ == "__main__":
    print("Starting Cello with async support test...")
    print("Test endpoints:")
    print("  GET http://127.0.0.1:8000/sync   - sync handler")
    print("  GET http://127.0.0.1:8000/async  - async handler")
    print("  GET http://127.0.0.1:8000/users/123 - async with params")
    print()
    app.run()
