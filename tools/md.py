#!/usr/bin/env python3
"""Renders one of docs/*.md into a page of the site, with no dependencies.

    python3 tools/md.py docs/RULES.md site/page.html "Rulesets" > _site/rules.html

Handles what the docs use: headings, paragraphs, `*` lists (continued by indented lines),
fenced code, tables, `code`, **bold**, *italics* and [links](url). Links to the other docs
(`RULES.md`, `ENGINE.md`) become links to their pages.
"""
import html
import re
import sys

PAGES = {"RULES.md": "rules.html", "ENGINE.md": "engine.html"}


def inline(text):
    out, pos = [], 0
    for m in re.finditer(r"`([^`]+)`", text):
        out.append(spans(text[pos:m.start()]))
        out.append("<code>" + html.escape(m.group(1)) + "</code>")
        pos = m.end()
    out.append(spans(text[pos:]))
    return "".join(out)


def spans(text):
    text = html.escape(text, quote=False)

    def link(m):
        url = m.group(2)
        page = url.split("#")[0]
        if page in PAGES:
            url = PAGES[page] + url[len(page):]
        return '<a href="%s">%s</a>' % (html.escape(url), m.group(1))

    text = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, text)
    text = re.sub(r"\*\*(.+?)\*\*", r"<b>\1</b>", text)
    text = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<i>\1</i>", text)
    return text


def slug(text):
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def render(md):
    lines = md.splitlines()
    out, toc, title = [], [], ""
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line.strip():
            i += 1
        elif line.startswith("```"):
            lang = line[3:].strip()
            j = i + 1
            while j < len(lines) and not lines[j].startswith("```"):
                j += 1
            code = html.escape("\n".join(lines[i + 1:j]))
            cls = ' class="lang-%s"' % lang if lang else ""
            out.append("<pre%s><code>%s</code></pre>" % (cls, code))
            i = j + 1
        elif line.startswith("#"):
            level = len(line) - len(line.lstrip("#"))
            text = line[level:].strip()
            if level == 1:
                title = text
                out.append("<h1>%s</h1>" % inline(text))
            else:
                sid = slug(text)
                if level == 2:
                    toc.append((sid, text))
                out.append('<h%d id="%s">%s</h%d>' % (level, sid, inline(text), level))
            i += 1
        elif line.startswith("|"):
            rows = []
            while i < len(lines) and lines[i].startswith("|"):
                cells = [c.strip() for c in lines[i].strip().strip("|").split("|")]
                if not all(re.fullmatch(r":?-+:?", c) for c in cells):
                    rows.append(cells)
                i += 1
            head = "".join("<th>%s</th>" % inline(c) for c in rows[0])
            body = "".join(
                "<tr>%s</tr>" % "".join("<td>%s</td>" % inline(c) for c in r) for r in rows[1:]
            )
            out.append('<div class="table"><table><thead><tr>%s</tr></thead><tbody>%s</tbody></table></div>' % (head, body))
        elif re.match(r"[*-] ", line):
            items = []
            while i < len(lines) and (re.match(r"[*-] ", lines[i]) or (lines[i].startswith("  ") and items)):
                if re.match(r"[*-] ", lines[i]):
                    items.append(lines[i][2:].strip())
                else:
                    items[-1] += " " + lines[i].strip()
                i += 1
            out.append("<ul>%s</ul>" % "".join("<li>%s</li>" % inline(t) for t in items))
        else:
            para = []
            while i < len(lines) and lines[i].strip() and not re.match(r"(```|#|\||[*-] )", lines[i]):
                para.append(lines[i].strip())
                i += 1
            out.append("<p>%s</p>" % inline(" ".join(para)))
    return title, toc, "\n".join(out)


def main():
    src, template, nav = sys.argv[1:4]
    with open(src, encoding="utf-8") as f:
        title, toc, body = render(f.read())
    with open(template, encoding="utf-8") as f:
        page = f.read()
    toc_html = "".join('<li><a href="#%s">%s</a></li>' % (sid, inline(t)) for sid, t in toc)
    page = page.replace("{{title}}", html.escape(title)).replace("{{toc}}", toc_html)
    page = page.replace('data-nav="%s"' % nav, 'data-nav="%s" class="on"' % nav)
    sys.stdout.write(page.replace("{{body}}", body))


if __name__ == "__main__":
    main()
