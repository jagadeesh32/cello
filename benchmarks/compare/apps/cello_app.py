"""Cello benchmark app - GET / returning JSON."""
import os
from cello import App

app = App()


@app.get("/")
def home(request):
    return {"message": "Hello, World!"}


if __name__ == "__main__":
    workers = int(os.environ.get("BENCH_WORKERS", os.cpu_count() or 1))
    app.run(host="127.0.0.1", port=8080, env="production", logs=False, workers=workers)
