"""Robyn benchmark app - GET / returning JSON."""
from robyn import Robyn

app = Robyn(__file__)


@app.get("/")
async def home():
    return {"message": "Hello, World!"}


if __name__ == "__main__":
    app.start(host="127.0.0.1", port=8080)
