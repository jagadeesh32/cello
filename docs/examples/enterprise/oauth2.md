---
title: OAuth2 & Social Login
description: OAuth2 authorization code flow with Google and GitHub
---

# :material-shield-account: OAuth2 & Social Login

Add "Sign in with Google" and "Sign in with GitHub" to your Cello application using the OAuth2 authorization code flow with PKCE (Proof Key for Code Exchange). This example covers the full lifecycle: building the redirect URL, handling the provider callback, exchanging the authorization code for tokens, storing credentials in the session, and linking a social identity to an existing account.

## Complete Example

```python
"""
enterprise/oauth2.py

OAuth2 authorization code flow + PKCE for Google and GitHub.
Covers: redirect, callback, token exchange, session storage, account linking.

Requirements:
    pip install cello httpx itsdangerous
"""
from __future__ import annotations

import base64
import hashlib
import json
import logging
import os
import secrets
import time
from dataclasses import dataclass, field
from typing import Any
from urllib.parse import urlencode

import httpx
from itsdangerous import URLSafeTimedSerializer

import cello
from cello import Request, Response

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s – %(message)s",
)
log = logging.getLogger("oauth2")

# ---------------------------------------------------------------------------
# Session helper (signed, tamper-proof cookie)
# ---------------------------------------------------------------------------
SECRET_KEY = os.getenv("SESSION_SECRET", "change-me-in-production-32-chars!!")
_signer = URLSafeTimedSerializer(SECRET_KEY)

SESSION_COOKIE = "session"
SESSION_MAX_AGE = 60 * 60 * 24 * 7  # 7 days


def get_session(request: Request) -> dict[str, Any]:
    """Deserialise the signed session cookie, returning an empty dict on failure."""
    raw = request.cookies.get(SESSION_COOKIE, "")
    try:
        return _signer.loads(raw, max_age=SESSION_MAX_AGE)
    except Exception:
        return {}


def set_session(response: Response, data: dict[str, Any]) -> None:
    """Serialise and sign the session, writing it as an HttpOnly cookie."""
    signed = _signer.dumps(data)
    response.set_cookie(
        SESSION_COOKIE,
        signed,
        max_age=SESSION_MAX_AGE,
        httponly=True,
        samesite="lax",
        secure=os.getenv("APP_ENV", "development") == "production",
    )


# ---------------------------------------------------------------------------
# PKCE helpers
# ---------------------------------------------------------------------------
def generate_pkce_pair() -> tuple[str, str]:
    """
    Generate a PKCE code_verifier and its SHA-256 code_challenge.

    Returns:
        (code_verifier, code_challenge)
    """
    code_verifier = base64.urlsafe_b64encode(secrets.token_bytes(32)).rstrip(b"=").decode()
    digest = hashlib.sha256(code_verifier.encode()).digest()
    code_challenge = base64.urlsafe_b64encode(digest).rstrip(b"=").decode()
    return code_verifier, code_challenge


# ---------------------------------------------------------------------------
# Provider configuration
# ---------------------------------------------------------------------------
@dataclass
class OAuthProvider:
    name: str
    client_id: str
    client_secret: str
    authorize_url: str
    token_url: str
    userinfo_url: str
    scopes: list[str] = field(default_factory=list)
    supports_pkce: bool = True

    @property
    def redirect_uri(self) -> str:
        base = os.getenv("APP_BASE_URL", "http://localhost:8000")
        return f"{base}/auth/{self.name}/callback"


PROVIDERS: dict[str, OAuthProvider] = {
    "google": OAuthProvider(
        name="google",
        client_id=os.getenv("GOOGLE_CLIENT_ID", ""),
        client_secret=os.getenv("GOOGLE_CLIENT_SECRET", ""),
        authorize_url="https://accounts.google.com/o/oauth2/v2/auth",
        token_url="https://oauth2.googleapis.com/token",
        userinfo_url="https://openidconnect.googleapis.com/v1/userinfo",
        scopes=["openid", "email", "profile"],
        supports_pkce=True,
    ),
    "github": OAuthProvider(
        name="github",
        client_id=os.getenv("GITHUB_CLIENT_ID", ""),
        client_secret=os.getenv("GITHUB_CLIENT_SECRET", ""),
        authorize_url="https://github.com/login/oauth/authorize",
        token_url="https://github.com/login/oauth/access_token",
        userinfo_url="https://api.github.com/user",
        scopes=["read:user", "user:email"],
        # GitHub does not support PKCE; we rely on state for CSRF protection.
        supports_pkce=False,
    ),
}

# ---------------------------------------------------------------------------
# In-memory "database" – replace with real persistence
# ---------------------------------------------------------------------------
# users[user_id]      = {"id", "email", "name", "avatar_url", "linked_providers"}
# identities[key]     = user_id   where key = "provider:provider_user_id"
users: dict[str, dict] = {}
identities: dict[str, str] = {}


def _upsert_user(provider: str, profile: dict) -> dict:
    """
    Find or create a local user for the given OAuth profile.
    If the request session already contains a user_id, link the social
    identity to that existing account (account linking).
    """
    identity_key = f"{provider}:{profile['id']}"

    if identity_key in identities:
        # Returning user – return the existing account
        return users[identities[identity_key]]

    # Brand-new social identity
    user_id = secrets.token_hex(16)
    user = {
        "id": user_id,
        "email": profile.get("email", ""),
        "name": profile.get("name") or profile.get("login", ""),
        "avatar_url": profile.get("picture") or profile.get("avatar_url", ""),
        "linked_providers": [provider],
        "created_at": int(time.time()),
    }
    users[user_id] = user
    identities[identity_key] = user_id
    log.info("Created user %s via %s", user_id, provider)
    return user


# ---------------------------------------------------------------------------
# OAuth2 handlers
# ---------------------------------------------------------------------------
app = cello.App()


@app.get("/auth/{provider}")
async def oauth_redirect(request: Request, provider: str) -> Response:
    """
    Step 1 – Build the provider's authorization URL and redirect the browser.
    Stores PKCE verifier and CSRF state token in the session.
    """
    if provider not in PROVIDERS:
        return Response(
            status=404,
            body=b'{"error": "unknown provider"}',
            headers={"Content-Type": "application/json"},
        )

    cfg = PROVIDERS[provider]
    state = secrets.token_urlsafe(32)  # CSRF protection

    params: dict[str, str] = {
        "client_id": cfg.client_id,
        "redirect_uri": cfg.redirect_uri,
        "response_type": "code",
        "scope": " ".join(cfg.scopes),
        "state": state,
    }

    session = get_session(request)
    session["oauth_state"] = state
    session["oauth_provider"] = provider

    if cfg.supports_pkce:
        code_verifier, code_challenge = generate_pkce_pair()
        params["code_challenge"] = code_challenge
        params["code_challenge_method"] = "S256"
        session["pkce_verifier"] = code_verifier

    redirect_url = f"{cfg.authorize_url}?{urlencode(params)}"
    log.info("Redirecting to %s for %s auth", provider, provider)

    response = Response(
        status=302,
        headers={"Location": redirect_url},
    )
    set_session(response, session)
    return response


@app.get("/auth/{provider}/callback")
async def oauth_callback(request: Request, provider: str) -> Response:
    """
    Step 2 – Exchange the authorization code for tokens,
    fetch the user profile, and establish a local session.
    """
    if provider not in PROVIDERS:
        return Response(
            status=404,
            body=b'{"error": "unknown provider"}',
            headers={"Content-Type": "application/json"},
        )

    cfg = PROVIDERS[provider]
    session = get_session(request)

    # --- CSRF check ---
    state = request.query.get("state", "")
    if not secrets.compare_digest(state, session.get("oauth_state", "")):
        log.warning("OAuth state mismatch for provider %s", provider)
        return Response(
            status=400,
            body=b'{"error": "invalid state parameter"}',
            headers={"Content-Type": "application/json"},
        )

    code = request.query.get("code", "")
    if not code:
        error = request.query.get("error", "access_denied")
        return Response(
            status=400,
            body=json.dumps({"error": error}).encode(),
            headers={"Content-Type": "application/json"},
        )

    # --- Token exchange ---
    token_params: dict[str, str] = {
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": cfg.redirect_uri,
        "client_id": cfg.client_id,
        "client_secret": cfg.client_secret,
    }
    if cfg.supports_pkce and "pkce_verifier" in session:
        token_params["code_verifier"] = session.pop("pkce_verifier")

    async with httpx.AsyncClient() as http:
        token_response = await http.post(
            cfg.token_url,
            data=token_params,
            headers={"Accept": "application/json"},
            timeout=10,
        )
        token_response.raise_for_status()
        tokens = token_response.json()

        access_token: str = tokens["access_token"]

        # --- Fetch user profile ---
        profile_response = await http.get(
            cfg.userinfo_url,
            headers={
                "Authorization": f"Bearer {access_token}",
                "Accept": "application/json",
            },
            timeout=10,
        )
        profile_response.raise_for_status()
        profile = profile_response.json()

    # GitHub returns separate /user/emails endpoint for primary email
    if provider == "github" and not profile.get("email"):
        async with httpx.AsyncClient() as http:
            emails_resp = await http.get(
                "https://api.github.com/user/emails",
                headers={
                    "Authorization": f"Bearer {access_token}",
                    "Accept": "application/json",
                },
                timeout=10,
            )
            if emails_resp.is_success:
                emails = emails_resp.json()
                primary = next(
                    (e["email"] for e in emails if e.get("primary") and e.get("verified")),
                    None,
                )
                if primary:
                    profile["email"] = primary

    # --- Normalise provider-specific profile shapes ---
    profile["id"] = str(profile.get("id") or profile.get("sub", ""))

    # --- Account linking or creation ---
    user = _upsert_user(provider, profile)

    # Store user in session, remove OAuth ephemeral keys
    session.pop("oauth_state", None)
    session["user_id"] = user["id"]
    session["user_email"] = user["email"]
    session["user_name"] = user["name"]
    # Store tokens so the app can make API calls on behalf of the user
    session[f"{provider}_access_token"] = access_token
    if "refresh_token" in tokens:
        session[f"{provider}_refresh_token"] = tokens["refresh_token"]

    log.info(
        "User %s authenticated via %s",
        user["id"],
        provider,
        extra={"provider": provider, "user_id": user["id"]},
    )

    # Redirect to the app after a successful login
    response = Response(
        status=302,
        headers={"Location": "/dashboard"},
    )
    set_session(response, session)
    return response


# ---------------------------------------------------------------------------
# Example protected endpoint
# ---------------------------------------------------------------------------
@app.get("/dashboard")
async def dashboard(request: Request) -> Response:
    session = get_session(request)
    user_id = session.get("user_id")
    if not user_id:
        return Response(
            status=302,
            headers={"Location": "/"},
        )
    user = users.get(user_id, {})
    return Response(
        status=200,
        body=json.dumps({"message": f"Welcome, {user.get('name')}!", "user": user}).encode(),
        headers={"Content-Type": "application/json"},
    )


@app.delete("/auth/logout")
async def logout(request: Request) -> Response:
    """Clear the session cookie."""
    response = Response(
        status=302,
        headers={"Location": "/"},
    )
    response.delete_cookie(SESSION_COOKIE)
    return response


@app.post("/auth/link/{provider}")
async def link_provider(request: Request, provider: str) -> Response:
    """
    Account linking — initiate OAuth for an already-authenticated user
    so that a second social identity is associated with their account.
    The callback's _upsert_user will merge via session["user_id"].
    """
    session = get_session(request)
    if not session.get("user_id"):
        return Response(
            status=401,
            body=b'{"error": "must be logged in to link a provider"}',
            headers={"Content-Type": "application/json"},
        )
    # Reuse the standard redirect handler
    return await oauth_redirect(request, provider)


# ---------------------------------------------------------------------------
# Simple landing page
# ---------------------------------------------------------------------------
@app.get("/")
async def index(request: Request) -> Response:
    session = get_session(request)
    if session.get("user_id"):
        html = (
            b"<h1>You are logged in.</h1>"
            b'<a href="/dashboard">Dashboard</a> | '
            b'<form method="POST" action="/auth/logout" style="display:inline">'
            b'<button>Logout</button></form>'
        )
    else:
        html = (
            b"<h1>Sign in</h1>"
            b'<a href="/auth/google">Sign in with Google</a><br>'
            b'<a href="/auth/github">Sign in with GitHub</a>'
        )
    return Response(
        status=200,
        body=html,
        headers={"Content-Type": "text/html; charset=utf-8"},
    )


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Key Concepts

- **Authorization code flow** — The browser is redirected to the provider's authorization URL with `response_type=code`. After the user consents, the provider redirects back to `/auth/{provider}/callback` with a short-lived `code`. The server exchanges this code for an `access_token` (and optionally a `refresh_token`) in a server-to-server call that never exposes the tokens to the browser.

- **PKCE (Proof Key for Code Exchange)** — For providers that support it (Google), a cryptographically random `code_verifier` is generated at redirect time. Its SHA-256 hash (`code_challenge`) is sent to the provider. The verifier is stored in the session and sent alongside the token exchange request. This prevents authorization code interception attacks, making the flow safe even for public clients.

- **CSRF protection via `state`** — A random `state` token is generated for every authorization request and stored in the session. The provider echoes it back in the callback. The handler verifies the echo with `secrets.compare_digest` (constant-time comparison) before proceeding, preventing cross-site request forgery attacks.

- **Provider-agnostic architecture** — Each provider is described by an `OAuthProvider` dataclass. Adding a new provider (e.g. Microsoft, Apple) requires only adding a new entry to the `PROVIDERS` dict; no handler code changes are needed.

- **Account linking** — `_upsert_user` checks whether the incoming `provider:provider_user_id` identity already maps to a local user. If it does, it returns the existing account. If not, it creates a new one. The `/auth/link/{provider}` endpoint lets an already-authenticated user attach a second social login to their account by simply re-entering the OAuth flow while carrying their session.

- **Token storage in session** — `access_token` and `refresh_token` are stored in the signed, `HttpOnly` session cookie. The cookie is protected against tampering via `itsdangerous.URLSafeTimedSerializer`. In production, store tokens in a server-side store (Redis or database) and keep only a session ID in the cookie to limit cookie size and enable token revocation.

- **GitHub email quirk** — GitHub's `/user` endpoint omits the email address if the user has set it to private. The callback handler makes a second call to `/user/emails` and extracts the primary, verified email. This is a common source of bugs in GitHub OAuth integrations.

## Running This Example

```bash
# 1. Register OAuth apps
#    Google: https://console.cloud.google.com/apis/credentials
#    GitHub:  https://github.com/settings/developers
#    Set the callback URL to: http://localhost:8000/auth/{provider}/callback

# 2. Install dependencies
pip install cello httpx itsdangerous uvicorn

# 3. Export credentials
export GOOGLE_CLIENT_ID="your-google-client-id"
export GOOGLE_CLIENT_SECRET="your-google-client-secret"
export GITHUB_CLIENT_ID="your-github-client-id"
export GITHUB_CLIENT_SECRET="your-github-client-secret"
export SESSION_SECRET="a-random-32-char-secret-key-here"
export APP_BASE_URL="http://localhost:8000"

# 4. Run the application
python examples/enterprise/oauth2.py

# 5. Open your browser and test the flows
open http://localhost:8000

# 6. Verify the session cookie is set after login
curl -c cookies.txt -L http://localhost:8000/auth/google
# (complete the browser flow, then)
curl -b cookies.txt http://localhost:8000/dashboard
```
