#!/usr/bin/env python3
"""Expand the mockup sources in src/*.html into standalone pages.

Shorthand (kept out of the output):
  {{wifi}} {{wifi|s16}}   -> <svg class="ic [s16]"><use href="#i-wifi"/></svg>
  {{app:files}} {{app:files md}} -> gradient app icon tile
  {{sprite}}              -> icons.svg inlined (Chromium blocks external <use> on file://)
"""
import pathlib, re
here = pathlib.Path(__file__).parent
sprite = (here / "icons.svg").read_text().strip()
APP_GLYPH = {"files": "folder", "terminal": "terminal", "web": "globe", "settings": "settings",
             "music": "music", "photos": "image", "code": "code", "calc": "calculator",
             "mail": "mail", "notes": "file", "calendar": "calendar", "store": "sparkles"}

def icon(name, cls=""):
    c = ("ic " + cls).strip()
    return f'<svg class="{c}"><use href="#i-{name}"/></svg>'

def expand(text):
    def app(m):
        name, extra = m.group(1), m.group(2) or ""
        cls = f"app-icon a-{name} {extra}".strip()
        return f'<span class="{cls}">{icon(APP_GLYPH[name])}</span>'
    text = re.sub(r"\{\{app:(\w+)(?: ([\w ]+))?\}\}", app, text)
    text = text.replace("{{sprite}}", sprite)
    return re.sub(r"\{\{([\w-]+)(?:\|([\w ]+))?\}\}", lambda m: icon(m.group(1), m.group(2) or ""), text)

for src in sorted((here / "src").glob("*.html")):
    (here / src.name).write_text(expand(src.read_text()))
    print("built", src.name)
