from cello import App
import asyncio
import time

app = App()

# Global state
DB_CONNECTION = None

@app.on_event("startup")
async def connect_db():
    global DB_CONNECTION
    print("🌱 Connecting to database (simulated)...")
    await asyncio.sleep(0.5)
    DB_CONNECTION = "Connected"
    print("✅ Database Connected")

@app.on_event("shutdown")
def close_db():
    print("🛑 Closing database connection...")
    time.sleep(0.1)
    print("✅ Database Closed")

@app.get("/")
def home(request):
    return {"db_status": DB_CONNECTION or "Disconnected"}

if __name__ == "__main__":
    print("🚀 Lifecycle Hooks Demo at http://127.0.0.1:8080")
    print("Watch console for startup/shutdown logs.")
    try:
        app.run(port=8080)
    except KeyboardInterrupt:
        pass
