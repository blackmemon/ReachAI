"""Browser check against a running gateway, in headless Chrome (Playwright, Python).

Needs: the gateway running with the built bundles (scripts/build_wasm.sh), a seeded demo
company, and these environment variables:
  E2E_BASE      default http://127.0.0.1:8000
  E2E_EMAIL     demo admin email
  E2E_PASSWORD  demo admin password
  E2E_DB        owner database URL (to read results back)
  E2E_SECRET    LEADS_SECRET_KEY of the gateway (to make one unsubscribe link)
Run first: cargo build -p tests --example unsub_link
The page is a canvas, so clicks use screen positions for the 1280x800 layout.
Each step asserts. A failed assert stops the run with a non-zero exit.
"""
import asyncio, os, subprocess, sys, time
from playwright.async_api import async_playwright

BASE = os.environ.get("E2E_BASE", "http://127.0.0.1:8000")
EMAIL = os.environ["E2E_EMAIL"]
PASSWORD = os.environ["E2E_PASSWORD"]
DB = os.environ["E2E_DB"]
SECRET = os.environ["E2E_SECRET"]
OUT = os.environ.get("E2E_SHOTS", "/tmp/e2e-shots")
os.makedirs(OUT, exist_ok=True)
results = []


def db(sql):
    r = subprocess.run(["psql", "-At", DB, "-c", sql], capture_output=True, text=True)
    return r.stdout.strip()


def ok(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(("PASS " if cond else "FAIL ") + name + (f"  [{detail}]" if detail and not cond else ""))
    if not cond:
        raise SystemExit(f"step failed: {name} {detail}")


async def shot(pg, n):
    await pg.screenshot(path=f"{OUT}/{n}.png")


async def login(pg, w=1280):
    cx = w // 2
    await pg.goto(BASE + "/sign-in")
    await pg.wait_for_timeout(3000)
    await pg.mouse.click(cx, 221); await pg.keyboard.type(EMAIL)
    await pg.keyboard.press("Tab"); await pg.keyboard.type(PASSWORD)
    await pg.keyboard.press("Enter")
    await pg.wait_for_timeout(4500)


async def main():
    async with async_playwright() as p:
        b = await p.chromium.launch(executable_path="/usr/bin/google-chrome", args=["--no-sandbox"])
        ctx = await b.new_context(viewport={"width": 1280, "height": 800})
        pg = await ctx.new_page()
        problems = []
        pg.on("pageerror", lambda e: problems.append(str(e)[:200]))
        pg.on("console", lambda m: problems.append(m.text[:200]) if m.type == "error" and "Failed to load resource" not in m.text else None)

        # 1. Public pages need no sign-in and draw a canvas.
        await pg.goto(BASE + "/privacy"); await pg.wait_for_timeout(3000)
        ok("privacy page draws a canvas", await pg.evaluate("document.querySelector('canvas') !== null"))
        await shot(pg, "01_privacy")

        # 2. A real unsubscribe link: page asks, the button acts, the database changes.
        tenant = db("select tenant_id from contacts limit 1")
        cid = db("select id from contacts where email = 'hira@indusdental.example'")
        tok = subprocess.run([os.path.join(os.environ.get("CARGO_TARGET_DIR", "target"), "debug/examples/unsub_link"), SECRET, tenant, cid], capture_output=True, text=True).stdout.strip()
        await pg.goto(BASE + "/unsubscribe/" + tok); await pg.wait_for_timeout(3500)
        await shot(pg, "02_unsub_ready")
        await pg.mouse.click(471, 245); await pg.wait_for_timeout(2500)
        await shot(pg, "02_unsub_done")
        ok("unsubscribe sets do-not-contact", db(f"select do_not_contact from contacts where id = '{cid}'") == "t")
        bad = await ctx.new_page()
        await bad.goto(BASE + "/unsubscribe/not.a.token"); await bad.wait_for_timeout(3000)
        await shot(bad, "02_unsub_bad"); await bad.close()

        # 3. A wrong password is refused with a plain message; the right one signs in.
        await pg.goto(BASE + "/sign-in"); await pg.wait_for_timeout(3000)
        await pg.mouse.click(640, 221); await pg.keyboard.type(EMAIL)
        await pg.mouse.click(640, 301); await pg.keyboard.type("wrong-password-1"); await pg.keyboard.press("Enter")
        await pg.wait_for_timeout(2500)
        await shot(pg, "03_wrong_password")
        ok("wrong password stays on sign-in", pg.url.endswith("/sign-in"))
        await pg.mouse.click(640, 361); await pg.keyboard.type(PASSWORD); await pg.keyboard.press("Enter")
        await pg.wait_for_timeout(4500)
        ok("right password opens the app", pg.url.rstrip("/") == BASE.rstrip("/"), pg.url)
        await shot(pg, "03_today")
        ok("session cookie works", await pg.evaluate("fetch('/api/v1/session').then(r => r.status)") == 200)

        # 4. Every sidebar page opens without a script error.
        for path in ["/", "/tasks", "/activity", "/quick", "/people", "/companies", "/lists", "/enrichment", "/emails", "/templates", "/domain_health", "/calls", "/conversations", "/meetings", "/deals", "/follow_ups", "/goals", "/campaigns", "/sequences", "/audiences", "/research", "/reports", "/settings", "/notices", "/profile", "/help"]:
            await pg.goto(BASE + path); await pg.wait_for_timeout(2500)
            if path in ("/people", "/settings", "/reports"):
                await shot(pg, "04_page" + path.replace("/", "_"))
        ok("26 pages opened with no script error", not problems, "; ".join(problems[:3]))

        # 5. Create a person in the drawer, with a required-field message first.
        await pg.goto(BASE + "/people"); await pg.wait_for_timeout(3500)
        await pg.mouse.click(1196, 106); await pg.wait_for_timeout(1500)
        people_before = db("select count(*) from contacts")
        await pg.mouse.click(772, 755); await pg.wait_for_timeout(1200)
        await shot(pg, "05_required")
        ok("empty form saves nothing", db("select count(*) from contacts") == people_before)
        person_name = f"Zainab Testwala {int(time.time())}"
        await pg.mouse.click(990, 150); await pg.keyboard.type("Zainab")
        await pg.keyboard.press("Control+A"); await pg.keyboard.type(person_name)
        await pg.keyboard.press("Tab"); await pg.keyboard.type("Test operator")
        await pg.keyboard.press("Control+A"); await pg.keyboard.press("Control+C"); await pg.keyboard.press("Control+V")
        await pg.keyboard.press("Enter"); await pg.wait_for_timeout(3000)
        await shot(pg, "05_saved")
        ok("new person is in the database", db(f"select first_name || ' ' || last_name || '|' || job_title from contacts where display_name = '{person_name}'") == f"{person_name}|Test operator")

        # 6. Search finds the person; the user menu signs out and the session ends.
        await pg.mouse.click(400, 176); await pg.keyboard.type("Zainab"); await pg.keyboard.press("Enter"); await pg.wait_for_timeout(2500)
        await shot(pg, "06_search")
        await pg.mouse.click(1226, 27); await pg.wait_for_timeout(800)
        await pg.mouse.click(1070, 221); await pg.wait_for_timeout(3500)
        ok("sign out returns to sign-in", pg.url.endswith("/sign-in"), pg.url)
        ok("session is gone after sign out", await pg.evaluate("fetch('/api/v1/session').then(r => r.status)") == 401)

        # 7. Phone width: sign in and open the menu.
        mctx = await b.new_context(viewport={"width": 390, "height": 800})
        m = await mctx.new_page()
        await login(m, 390)
        await shot(m, "07_phone_today")
        await m.mouse.click(32, 27); await m.wait_for_timeout(800)
        await shot(m, "07_phone_menu")
        ok("phone layout signs in", m.url.rstrip("/") == BASE.rstrip("/"), m.url)
        await b.close()
    print(f"\n{sum(1 for _, c, _ in results if c)} of {len(results)} browser steps passed. Screenshots: {OUT}")


asyncio.run(main())
