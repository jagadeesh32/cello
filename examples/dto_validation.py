from cello import App, Response

try:
    from pydantic import BaseModel, EmailStr
    HAS_PYDANTIC = True
except ImportError:
    HAS_PYDANTIC = False
    print("Warning: Pydantic not installed. DTO example will not work fully.")

app = App()

if HAS_PYDANTIC:
    class CreateUserDTO(BaseModel):
        username: str
        email: EmailStr
        age: int

    @app.post("/users")
    def create_user(request, user: CreateUserDTO):
        # user is already validated instance of CreateUserDTO
        return {
            "status": "created",
            "user": user.dict()
        }
else:
    @app.post("/users")
    def create_user(request):
        return {"error": "Pydantic missing"}

if __name__ == "__main__":
    print("🚀 DTO Validation Demo at http://127.0.0.1:8080")
    print("Try sending invalid JSON to /users to see 422 error.")
    app.run(port=8080)
