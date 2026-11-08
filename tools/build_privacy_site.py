#!/usr/bin/env python3
"""Build the public Harbor LM privacy page from the in-app policy text.

The app shows apps/harbor_app/assets/privacy/privacy_policy.md under
Settings -> Privacy Policy; this renders the SAME file to static HTML for
Cloudflare Pages, so the two cannot drift.

    python3 tools/build_privacy_site.py [--out DIR]   # default: build/privacy_site
    npx wrangler pages deploy DIR --project-name harbor-lm --branch main

The Markdown subset matches parsePrivacyPolicy() in
apps/harbor_app/lib/surfaces/privacy_policy_page.dart: '#' title, '##'
headings, '- ' bullets, paragraphs. URLs and e-mail addresses become links.
"""
import argparse
import html
import pathlib
import re

REPO = pathlib.Path(__file__).resolve().parent.parent
SOURCE = REPO / "apps/harbor_app/assets/privacy/privacy_policy.md"

LINK = re.compile(r"(https://[^\s,;)]+[^\s,;).]|[\w.+-]+@[\w-]+\.[\w.]+\w)")


def inline(text: str) -> str:
    out, last = [], 0
    for m in LINK.finditer(text):
        out.append(html.escape(text[last:m.start()]))
        target = m.group(0)
        href = target if target.startswith("https://") else f"mailto:{target}"
        out.append(f'<a href="{html.escape(href)}">{html.escape(target)}</a>')
        last = m.end()
    out.append(html.escape(text[last:]))
    return "".join(out)


def render(md: str) -> tuple[str, str]:
    title, parts, in_list = "Privacy Policy", [], False
    for raw in md.splitlines():
        line = raw.rstrip()
        if not line:
            continue
        if line.startswith("- "):
            if not in_list:
                parts.append("<ul>")
                in_list = True
            parts.append(f"<li>{inline(line[2:])}</li>")
            continue
        if in_list:
            parts.append("</ul>")
            in_list = False
        if line.startswith("## "):
            parts.append(f"<h2>{inline(line[3:])}</h2>")
        elif line.startswith("# "):
            title = line[2:]
            parts.append(f"<h1>{inline(title)}</h1>")
        else:
            parts.append(f"<p>{inline(line)}</p>")
    if in_list:
        parts.append("</ul>")
    return title, "\n".join(parts)


PAGE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="How Harbor LM by Cogniwiss Labs handles your information: on-device AI, no accounts, no analytics.">
<style>
:root {{ --bg:#f7f9fc; --ink:#0f1b2a; --muted:#4a5868; --rule:#d9e0e8; --link:#0b5cad; }}
@media (prefers-color-scheme: dark) {{
  :root {{ --bg:#08131f; --ink:#e6edf5; --muted:#9fb0c2; --rule:#1f3045; --link:#7db7ff; }}
}}
* {{ box-sizing: border-box; }}
body {{ margin:0; background:var(--bg); color:var(--ink);
  font: 16px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }}
main {{ max-width: 720px; margin: 0 auto; padding: 40px 16px 64px; }}
h1 {{ font-size: 1.9rem; line-height: 1.2; margin: 0 0 16px; }}
h2 {{ font-size: 1.2rem; margin: 36px 0 8px; padding-top: 20px; border-top: 1px solid var(--rule); }}
p, li {{ overflow-wrap: anywhere; }}
ul {{ padding-left: 1.25em; }}
li {{ margin: 6px 0; }}
a {{ color: var(--link); }}
footer {{ color: var(--muted); font-size: .875rem; margin-top: 48px; }}
</style>
</head>
<body>
<main>
{body}
<footer>Harbor LM · Cogniwiss Labs · <a href="mailto:privacy@cogniwiss.com">privacy@cogniwiss.com</a></footer>
</main>
</body>
</html>
"""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(REPO / "build/privacy_site"))
    out = pathlib.Path(ap.parse_args().out)
    title, body = render(SOURCE.read_text(encoding="utf-8"))
    page = PAGE.format(title=html.escape(title), body=body)
    (out / "privacy").mkdir(parents=True, exist_ok=True)
    (out / "privacy" / "index.html").write_text(page, encoding="utf-8")
    # The bare domain shows the policy too; it is the only page.
    (out / "index.html").write_text(page, encoding="utf-8")
    (out / "_headers").write_text(
        "/*\n  X-Content-Type-Options: nosniff\n  Referrer-Policy: no-referrer\n"
        "  Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\n",
        encoding="utf-8",
    )
    print(f"wrote {out}/privacy/index.html")


if __name__ == "__main__":
    main()
