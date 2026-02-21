"""FastAPI benchmark app - GET / returning JSON (served via Granian)."""
from fastapi import FastAPI

app = FastAPI()


@app.get("/")
async def home():
    return {"message": "Hello, World!"}
