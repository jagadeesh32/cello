"""
MiniJinja Email Templates Example — Cello v1.1.0

Shows how to use a standalone MiniJinjaEngine (no App required) to render
both plain-text and HTML email templates, then serve previews over HTTP.

Patterns covered:
  - Standalone MiniJinjaEngine outside of App
  - Plain-text templates (no auto-escape)
  - HTML email templates (auto-escape for the main engine)
  - Shared layout for HTML emails via template inheritance
  - Rendering the same data in two formats (text vs HTML)
  - Preview endpoints so you can eyeball emails in a browser

Run:
    python examples/minijinja_emails.py
Then visit:
    http://localhost:8085/preview/welcome/Alice
    http://localhost:8085/preview/reset/Bob
    http://localhost:8085/preview/invoice/42
    http://localhost:8085/text/welcome/Alice     ← raw plain-text
"""

import os
import tempfile

from cello import App, MiniJinjaEngine, Response

TEMPLATE_DIR = tempfile.mkdtemp(prefix="cello_email_")


def tpl(name, content):
    path = os.path.join(TEMPLATE_DIR, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content)


# ---------------------------------------------------------------------------
# Shared HTML email layout
# ---------------------------------------------------------------------------
tpl("emails/html/base_email.html", """\
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{% block subject %}Email{% endblock %}</title>
</head>
<body style="margin:0;padding:0;background:#f4f4f4;font-family:Arial,sans-serif;">
  <table width="100%" cellpadding="0" cellspacing="0">
    <tr>
      <td align="center" style="padding:30px 0;">
        <table width="600" cellpadding="0" cellspacing="0"
               style="background:#fff;border-radius:8px;
                      box-shadow:0 2px 8px rgba(0,0,0,.08);overflow:hidden;">
          <!-- Header -->
          <tr>
            <td style="background:#1a1a2e;padding:24px 32px;">
              <span style="color:#a8d8ea;font-size:1.3rem;font-weight:bold;">
                {{ company_name }}
              </span>
            </td>
          </tr>
          <!-- Body -->
          <tr>
            <td style="padding:32px;">
              {% block body %}{% endblock %}
            </td>
          </tr>
          <!-- Footer -->
          <tr>
            <td style="background:#f9f9f9;padding:16px 32px;
                       font-size:0.78rem;color:#999;border-top:1px solid #eee;">
              © {{ year }} {{ company_name }} ·
              <a href="{{ unsubscribe_url }}" style="color:#999;">Unsubscribe</a>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>
""")

# ---------------------------------------------------------------------------
# Welcome email
# ---------------------------------------------------------------------------
tpl("emails/html/welcome.html", """\
{% extends "emails/html/base_email.html" %}
{% block subject %}Welcome to {{ company_name }}, {{ name }}!{% endblock %}

{% block body %}
<h2 style="margin-top:0;color:#1a1a2e;">Welcome aboard, {{ name }}! 🎉</h2>
<p>Your account has been created. Here's what to do next:</p>
<ol>
  <li>Confirm your email address</li>
  <li>Complete your profile</li>
  <li>Explore the dashboard</li>
</ol>
<p style="margin:1.5rem 0;">
  <a href="{{ confirm_url }}"
     style="background:#1a73e8;color:#fff;padding:10px 24px;
            border-radius:4px;text-decoration:none;font-weight:bold;">
    Confirm Email
  </a>
</p>
<p style="color:#888;font-size:0.85rem;">
  This link expires in 24 hours. If you didn't register, ignore this email.
</p>
{% endblock %}
""")

tpl("emails/text/welcome.txt", """\
Welcome to {{ company_name }}, {{ name }}!

Your account has been created. Here's what to do next:
  1. Confirm your email address
  2. Complete your profile
  3. Explore the dashboard

Confirm your email:
  {{ confirm_url }}

This link expires in 24 hours.
If you didn't register, please ignore this email.

---
© {{ year }} {{ company_name }}
""")

# ---------------------------------------------------------------------------
# Password reset email
# ---------------------------------------------------------------------------
tpl("emails/html/password_reset.html", """\
{% extends "emails/html/base_email.html" %}
{% block subject %}Reset your {{ company_name }} password{% endblock %}

{% block body %}
<h2 style="margin-top:0;color:#1a1a2e;">Password Reset Request</h2>
<p>Hi {{ name }},</p>
<p>We received a request to reset your password.
   Click the button below to choose a new one.</p>
<p style="margin:1.5rem 0;">
  <a href="{{ reset_url }}"
     style="background:#d32f2f;color:#fff;padding:10px 24px;
            border-radius:4px;text-decoration:none;font-weight:bold;">
    Reset Password
  </a>
</p>
<p style="color:#888;font-size:0.85rem;">
  This link expires in <strong>{{ expires_minutes }} minutes</strong>.
  If you didn't request this, you can safely ignore this email —
  your password will not change.
</p>
{% endblock %}
""")

tpl("emails/text/password_reset.txt", """\
Password Reset Request
======================

Hi {{ name }},

We received a request to reset your {{ company_name }} password.

Reset your password here:
  {{ reset_url }}

This link expires in {{ expires_minutes }} minutes.

If you didn't request this, ignore this email — your password is unchanged.

---
© {{ year }} {{ company_name }}
""")

# ---------------------------------------------------------------------------
# Invoice email
# ---------------------------------------------------------------------------
tpl("emails/html/invoice.html", """\
{% extends "emails/html/base_email.html" %}
{% block subject %}Invoice #{{ invoice.id }} — {{ company_name }}{% endblock %}

{% block body %}
<h2 style="margin-top:0;color:#1a1a2e;">Invoice #{{ invoice.id }}</h2>
<p>Hi {{ customer_name }}, here's your invoice for {{ invoice.period }}.</p>

<table width="100%" cellpadding="0" cellspacing="0"
       style="margin:1rem 0;font-size:0.9rem;">
  <tr style="background:#f5f5f5;">
    <th style="padding:8px;text-align:left;border-bottom:2px solid #ddd;">Item</th>
    <th style="padding:8px;text-align:right;border-bottom:2px solid #ddd;">Amount</th>
  </tr>
  {% for line in invoice.lines %}
  <tr>
    <td style="padding:8px;border-bottom:1px solid #eee;">{{ line.description }}</td>
    <td style="padding:8px;text-align:right;border-bottom:1px solid #eee;">
      ${{ "%.2f" | format(line.amount) }}
    </td>
  </tr>
  {% endfor %}
  <tr>
    <td style="padding:10px 8px;font-weight:bold;">Total</td>
    <td style="padding:10px 8px;text-align:right;font-weight:bold;font-size:1.1rem;">
      ${{ "%.2f" | format(invoice.total) }}
    </td>
  </tr>
</table>

{% if invoice.paid %}
<p style="color:#2e7d32;font-weight:bold;">✔ Paid — thank you!</p>
{% else %}
<p style="margin:1.5rem 0;">
  <a href="{{ pay_url }}"
     style="background:#1a73e8;color:#fff;padding:10px 24px;
            border-radius:4px;text-decoration:none;font-weight:bold;">
    Pay Now
  </a>
</p>
{% endif %}
{% endblock %}
""")

tpl("emails/text/invoice.txt", """\
Invoice #{{ invoice.id }}
========================
Customer: {{ customer_name }}
Period:   {{ invoice.period }}

Items:
{% for line in invoice.lines %}
  {{ line.description | ljust(30) }} ${{ "%.2f" | format(line.amount) }}
{% endfor %}
  ----------------------------------------
  Total                              ${{ "%.2f" | format(invoice.total) }}

{% if invoice.paid %}
PAID — thank you!
{% else %}
Pay online: {{ pay_url }}
{% endif %}

---
© {{ year }} {{ company_name }}
""")

# ---------------------------------------------------------------------------
# Two engines: HTML (auto-escape on) and text (auto-escape off)
# ---------------------------------------------------------------------------
html_engine = MiniJinjaEngine(template_dir=TEMPLATE_DIR, auto_escape=True)
text_engine  = MiniJinjaEngine(template_dir=TEMPLATE_DIR, auto_escape=False)

# Shared globals
GLOBALS = {
    "company_name":    "Cello Corp",
    "year":            2026,
    "unsubscribe_url": "https://example.com/unsubscribe",
}
html_engine.add_globals(GLOBALS)
text_engine.add_globals(GLOBALS)

# ---------------------------------------------------------------------------
# Sample data builders
# ---------------------------------------------------------------------------

def _welcome_ctx(name):
    return {
        "name":        name,
        "confirm_url": f"https://example.com/confirm/{name.lower()}?token=abc123",
    }


def _reset_ctx(name):
    return {
        "name":            name,
        "reset_url":       f"https://example.com/reset/{name.lower()}?token=xyz789",
        "expires_minutes": 30,
    }


def _invoice_ctx(invoice_id):
    lines = [
        {"description": "Cello Pro Plan (monthly)", "amount": 49.00},
        {"description": "Extra worker seats × 3",   "amount": 15.00},
        {"description": "Bandwidth overage (10 GB)", "amount":  2.50},
    ]
    return {
        "customer_name": "Alice Wonderland",
        "invoice": {
            "id":     invoice_id,
            "period": "March 2026",
            "lines":  lines,
            "total":  sum(l["amount"] for l in lines),
            "paid":   invoice_id % 2 == 0,   # even IDs are "paid"
        },
        "pay_url": f"https://example.com/pay/{invoice_id}",
    }


# ---------------------------------------------------------------------------
# App — preview endpoints
# ---------------------------------------------------------------------------
app = App()
# The app itself only serves preview pages, no template engine needed on it
app.enable_templates(template_dir=TEMPLATE_DIR, auto_escape=True)


@app.get("/preview/welcome/{name}")
def preview_welcome(request):
    ctx = _welcome_ctx(request.params["name"])
    html = html_engine.render("emails/html/welcome.html", ctx)
    return Response.html(html)


@app.get("/text/welcome/{name}")
def text_welcome(request):
    ctx = _welcome_ctx(request.params["name"])
    text = text_engine.render("emails/text/welcome.txt", ctx)
    return Response.text(text)


@app.get("/preview/reset/{name}")
def preview_reset(request):
    ctx = _reset_ctx(request.params["name"])
    html = html_engine.render("emails/html/password_reset.html", ctx)
    return Response.html(html)


@app.get("/text/reset/{name}")
def text_reset(request):
    ctx = _reset_ctx(request.params["name"])
    text = text_engine.render("emails/text/password_reset.txt", ctx)
    return Response.text(text)


@app.get("/preview/invoice/{id}")
def preview_invoice(request):
    try:
        inv_id = int(request.params["id"])
    except ValueError:
        return {"error": "id must be an integer"}, 400
    ctx = _invoice_ctx(inv_id)
    html = html_engine.render("emails/html/invoice.html", ctx)
    return Response.html(html)


@app.get("/text/invoice/{id}")
def text_invoice(request):
    try:
        inv_id = int(request.params["id"])
    except ValueError:
        return {"error": "id must be an integer"}, 400
    ctx = _invoice_ctx(inv_id)
    text = text_engine.render("emails/text/invoice.txt", ctx)
    return Response.text(text)


if __name__ == "__main__":
    print(f"Templates: {TEMPLATE_DIR}")
    print("Listening on http://localhost:8085")
    print("  /preview/welcome/Alice")
    print("  /preview/reset/Bob")
    print("  /preview/invoice/42   (paid)")
    print("  /preview/invoice/7    (unpaid)")
    print("  /text/welcome/Alice   (plain-text)")
    app.run(port=8085)
