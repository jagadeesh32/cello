"""BlackSheep benchmark app - GET / returning JSON (served via Granian)."""
from blacksheep import Application, json

app = Application(show_error_details=False)


@app.router.get("/")
async def home():
    return json({"message": "Hello, World!"})
