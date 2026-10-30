#!/usr/bin/env python3
"""Build the PCBCraft brand masters and every platform icon.

Run through `assets/brand/build.sh`, which provides rsvg-convert, fontTools and
Pillow in a container. Output is deterministic: the same script gives the same
files, and the files are committed so a normal build needs no rasteriser.

Mark: concept 2, Layer stack (#686), finished in #687. Three offset plates
(solder mask green, copper, silkscreen) with a pin-1 chamfer, and a via drilled
through the stack so each plate shows a ring of the plate below it.

Masters are written to assets/brand/. Platform icons and previews are written
to assets/brand/generated/. Wordmark: IBM Plex Sans SemiBold, SIL OFL 1.1,
outlined to paths from assets/brand/fonts/IBMPlexSans-Variable.ttf.
"""
import os
import shutil
import struct
import subprocess
import sys
import json

HERE = os.path.dirname(os.path.abspath(__file__))
BRAND = os.path.dirname(HERE)
GEN = os.path.join(BRAND, "generated")
APP_ID = "io.github.tangency_labs.pcbcraft"

# Palette. Dark-page values and light-page values per #686 section 2.
TILE = "#0b1a16"
DARK = dict(green="#1f7a5f", copper="#d98e4f", top="#f1eee4", text="#f1eee4")
LIGHT = dict(green="#14503f", copper="#a8571f", top="#1b2a25", text="#1b2a25")
MONO = dict(green="#000", copper="#000", top="#000", text="#000")

# Mark geometry in a 392 x 392 box (plate, offset, chamfer, via centre).
BOX = 392
GEOM_FULL = dict(s=264, d=64, c=72, cx=196, cy=196, box=392)
# 32 px grid: whole-pixel plates, 3 px visible strips.
GEOM_32 = dict(s=16, d=4, c=4, cx=14, cy=10, box=24)
# 16 px grid: whole-pixel plates, 2 px visible strips.
GEOM_16 = dict(s=8, d=3, c=2, cx=9, cy=5, box=14)


def n(v):
    s = ("%.2f" % v).rstrip("0").rstrip(".")
    return s if s != "-0" else "0"


def plate_origin(g, k):
    return g["d"] * k, g["box"] - g["s"] - g["d"] * k


def plate_d(g, k):
    """Chamfered square (pin-1 corner top left) for plate k, 0 bottom."""
    x, y = plate_origin(g, k)
    s, c = g["s"], g["c"]
    return "M%s %sL%s %sH%sV%sH%sZ" % (n(x), n(y + c), n(x + c), n(y), n(x + s), n(y + s), n(x))


def via_centre(g):
    """Via position: inside the top plate, over the copper plate."""
    return g["cx"], g["cy"]


def circle_d(cx, cy, r):
    return "M%s %sa%s %s 0 1 0 %s 0a%s %s 0 1 0 %s 0Z" % (
        n(cx - r), n(cy), n(r), n(r), n(2 * r), n(r), n(r), n(-2 * r))


def mark_body(g, pal, gap, holes, hole_gap, uid):
    """SVG elements for the mark in the box 0..g["box"], no wrapper.

    holes: radii of the via hole in plates (bottom, middle, top); None for none.
    gap: transparent gap left around every upper plate. hole_gap: same around
    the hole edges. Gaps are cut with masks, so the mark has no background.
    """
    cx, cy = via_centre(g)
    cols = [pal["green"], pal["copper"], pal["top"]]
    defs, body = [], []
    for k in range(3):
        d = plate_d(g, k)
        if holes[k]:
            d += circle_d(cx, cy, holes[k])
        cuts = []
        for j in range(k + 1, 3):
            outline = plate_d(g, j)
            cut = '<path d="%s" fill="#000" stroke="#000" stroke-width="%s" stroke-linejoin="miter"/>' % (
                outline, n(2 * gap))
            if holes[j]:
                # Keep the hole open for the plate below, with its own gap.
                cut = ('<path d="%s" fill="none" stroke="#000" stroke-width="%s" stroke-linejoin="miter"/>'
                       '<path d="%s" fill="#000" fill-rule="evenodd"/>' % (
                           outline, n(2 * gap), outline + circle_d(cx, cy, holes[j])))
                if hole_gap:
                    cut += '<circle cx="%s" cy="%s" r="%s" fill="none" stroke="#000" stroke-width="%s"/>' % (
                        n(cx), n(cy), n(holes[j]), n(2 * hole_gap))
            cuts.append(cut)
        attrs = 'd="%s" fill="%s"' % (d, cols[k])
        if holes[k]:
            attrs += ' fill-rule="evenodd"'
        if cuts:
            mid = "%s-m%d" % (uid, k)
            defs.append(
                '<mask id="%s" maskUnits="userSpaceOnUse" x="-8" y="-8" width="%s" height="%s">'
                '<rect x="-8" y="-8" width="%s" height="%s" fill="#fff"/>%s</mask>' % (
                    mid, n(g["box"] + 16), n(g["box"] + 16), n(g["box"] + 16), n(g["box"] + 16), "".join(cuts)))
            attrs += ' mask="url(#%s)"' % mid
        body.append("<path %s/>" % attrs)
    return "<defs>%s</defs>%s" % ("".join(defs), "".join(body))


# Variants: (geometry, gap, holes bottom/middle/top, hole gap)
FULL_COLOUR = (GEOM_FULL, 8, (12, 30, 52), 8)
FULL_MONO = (GEOM_FULL, 16, (None, None, 64), 16)
SMALL_32 = (GEOM_32, 1, (None, None, 3.5), 1)
SMALL_16 = (GEOM_16, 1, (None, None, 1.2), 0)


def mark(pal, variant, uid):
    g, gap, holes, hg = variant
    return mark_body(g, pal, gap, holes, hg, uid)


def svg(w, h, view, inner, title, extra=""):
    return ('<?xml version="1.0" encoding="UTF-8"?>\n'
            '<svg xmlns="http://www.w3.org/2000/svg" width="%s" height="%s" viewBox="%s" role="img" '
            'aria-label="%s"%s>\n<title>%s</title>\n%s\n</svg>\n') % (n(w), n(h), view, title, extra, title, inner)


def tile_icon(variant, size, radius, scale=1.0, uid="i"):
    """Icon with the dark tile. size is the canvas, scale shrinks the mark."""
    g = variant[0]
    k = size / 512.0 if g is GEOM_FULL else size / (32.0 if g is GEOM_32 else 16.0)
    mark_px = g["box"] * k * scale if g is not GEOM_FULL else 392 * k * scale
    off = (size - mark_px) / 2.0
    tile = '<rect width="%s" height="%s" rx="%s" fill="%s"/>' % (n(size), n(size), n(radius), TILE)
    unit = mark_px / g["box"]
    inner = tile + '<g transform="translate(%s %s) scale(%s)">%s</g>' % (
        n(off), n(off), n(unit), mark(DARK, variant, uid))
    return inner


# ---- wordmark -------------------------------------------------------------

def outline_wordmark():
    from fontTools.ttLib import TTFont
    from fontTools.varLib import instancer
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen

    font = TTFont(os.path.join(BRAND, "fonts", "IBMPlexSans-Variable.ttf"))
    font = instancer.instantiateVariableFont(font, {"wght": 600}, inplace=False)
    upm = font["head"].unitsPerEm
    cmap = font.getBestCmap()
    gs = font.getGlyphSet()
    hmtx = font["hmtx"]
    cap = font["OS/2"].sCapHeight
    # Cap height 150 units against the 392 unit mark.
    scale = 150.0 / cap

    def run(text, x):
        out = []
        for ch in text:
            gn = cmap[ord(ch)]
            pen = SVGPathPen(gs, ntos=lambda v: n(v))
            tp = TransformPen(pen, (scale, 0, 0, -scale, x, 0))
            gs[gn].draw(tp)
            out.append(pen.getCommands())
            x += hmtx[gn][0] * scale
        return " ".join(out), x

    pcb, x = run("PCB", 0)
    craft, x = run("Craft", x)
    return pcb, craft, x, cap * scale


def wordmark_svg(pal, parts, uid):
    pcb, craft, width, cap = parts
    copper = pal["copper"] if pal is not MONO else "#000"
    return ('<path d="%s" fill="%s"/><path d="%s" fill="%s"/>' % (pcb, pal["text"], craft, copper)), width, cap


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def render(src, dst, size):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), src, "-o", dst], check=True)


def render_w(src, dst, width):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    subprocess.run(["rsvg-convert", "-w", str(width), src, "-o", dst], check=True)


def png_size(path):
    with open(path, "rb") as f:
        head = f.read(24)
    return struct.unpack(">II", head[16:24])


def make_ico(pngs, dst):
    data = [open(p, "rb").read() for p in pngs]
    out = struct.pack("<HHH", 0, 1, len(pngs))
    offset = 6 + 16 * len(pngs)
    entries = b""
    for p, d in zip(pngs, data):
        w, h = png_size(p)
        entries += struct.pack("<BBBBHHII", 0 if w >= 256 else w, 0 if h >= 256 else h, 0, 0, 1, 32, len(d), offset)
        offset += len(d)
    with open(dst, "wb") as f:
        f.write(out + entries + b"".join(data))


def make_icns(entries, dst):
    body = b""
    for typ, p in entries:
        d = open(p, "rb").read()
        body += typ.encode() + struct.pack(">I", len(d) + 8) + d
    with open(dst, "wb") as f:
        f.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main():
    parts = outline_wordmark()

    # --- masters ---------------------------------------------------------
    m = {}
    m["pcbcraft-icon.svg"] = svg(512, 512, "0 0 512 512", tile_icon(FULL_COLOUR, 512, 96, uid="a"),
                                 "PCBCraft app icon")
    m["pcbcraft-icon-32.svg"] = svg(32, 32, "0 0 32 32", tile_icon(SMALL_32, 32, 6, uid="b"),
                                    "PCBCraft app icon, 32 px version")
    m["pcbcraft-icon-16.svg"] = svg(16, 16, "0 0 16 16", tile_icon(SMALL_16, 16, 3, uid="c"),
                                    "PCBCraft app icon, 16 px version")
    for name, pal, label in (("dark", DARK, "for dark backgrounds"), ("light", LIGHT, "for light backgrounds")):
        m["pcbcraft-mark-%s.svg" % name] = svg(392, 392, "0 0 392 392", mark(pal, FULL_COLOUR, "m" + name),
                                               "PCBCraft mark, %s" % label)
    m["pcbcraft-mark-mono.svg"] = svg(392, 392, "0 0 392 392", mark(MONO, FULL_MONO, "mm"),
                                      "PCBCraft mark, one colour")

    for name, pal, vr, label in (("dark", DARK, FULL_COLOUR, "for dark backgrounds"),
                                 ("light", LIGHT, FULL_COLOUR, "for light backgrounds"),
                                 ("mono", MONO, FULL_MONO, "one colour")):
        word, width, cap = wordmark_svg(pal, parts, name)
        # Wordmark alone: baseline at y = cap, so the box is the cap height.
        pad = 0
        m["pcbcraft-wordmark-%s.svg" % name] = svg(
            width, cap, "0 0 %s %s" % (n(width), n(cap)),
            '<g transform="translate(0 %s)">%s</g>' % (n(cap), word), "PCBCraft wordmark, %s" % label)
        gapx = 56
        total = 392 + gapx + width
        base = 196 + cap / 2.0
        lock = (mark(pal, vr, "l" + name) +
                '<g transform="translate(%s %s)">%s</g>' % (n(392 + gapx), n(base), word))
        m["pcbcraft-logo-%s.svg" % name] = svg(total, 392, "0 0 %s 392" % n(total), lock,
                                               "PCBCraft logo, %s" % label)
    for name, text in m.items():
        write(os.path.join(BRAND, name), text)

    # --- generated platform icons ----------------------------------------
    if os.path.isdir(GEN):
        shutil.rmtree(GEN)
    os.makedirs(GEN)
    work = os.path.join(GEN, ".work")
    os.makedirs(work)

    def src_for(size):
        if size <= 16:
            return os.path.join(BRAND, "pcbcraft-icon-16.svg")
        if size <= 32:
            return os.path.join(BRAND, "pcbcraft-icon-32.svg")
        return os.path.join(BRAND, "pcbcraft-icon.svg")

    lin = os.path.join(GEN, "linux", "hicolor")
    for s in (16, 24, 32, 48, 64, 128, 256, 512):
        render(src_for(s), os.path.join(lin, "%dx%d" % (s, s), "apps", APP_ID + ".png"), s)
    scal = os.path.join(lin, "scalable", "apps")
    os.makedirs(scal)
    shutil.copy(os.path.join(BRAND, "pcbcraft-icon.svg"), os.path.join(scal, APP_ID + ".svg"))

    # Windows
    wpngs = []
    for s in (16, 24, 32, 48, 64, 128, 256):
        p = os.path.join(work, "win-%d.png" % s)
        render(src_for(s), p, s)
        wpngs.append(p)
    os.makedirs(os.path.join(GEN, "windows"))
    make_ico(wpngs, os.path.join(GEN, "windows", "pcbcraft.ico"))

    # macOS: rounded tile at 80 percent of the canvas, transparent margin.
    def mac_svg(variant, canvas):
        g = variant[0]
        grid = 512 if g is GEOM_FULL else (32 if g is GEOM_32 else 16)
        r = {512: 96, 32: 6, 16: 3}[grid]
        inner = tile_icon(variant, grid, r, uid="x")
        pad = grid * 0.1
        body = '<g transform="translate(%s %s) scale(0.8)">%s</g>' % (n(pad), n(pad), inner)
        return svg(grid, grid, "0 0 %d %d" % (grid, grid), body, "PCBCraft app icon")

    def mac_png(px, name):
        variant = SMALL_16 if px <= 16 else (SMALL_32 if px <= 32 else FULL_COLOUR)
        s = os.path.join(work, "mac-%s.svg" % name)
        write(s, mac_svg(variant, px))
        p = os.path.join(work, "mac-%s.png" % name)
        render(s, p, px)
        return p

    ents = []
    for typ, px in (("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128), ("ic08", 256), ("ic09", 512),
                    ("ic10", 1024), ("ic11", 32), ("ic12", 64), ("ic13", 256), ("ic14", 512)):
        ents.append((typ, mac_png(px, typ)))
    os.makedirs(os.path.join(GEN, "macos"))
    make_icns(ents, os.path.join(GEN, "macos", "pcbcraft.icns"))

    # Web
    web = os.path.join(GEN, "web")
    os.makedirs(web)
    fav = []
    for s in (16, 32, 48):
        p = os.path.join(work, "fav-%d.png" % s)
        render(src_for(s), p, s)
        fav.append(p)
    make_ico(fav, os.path.join(web, "favicon.ico"))
    shutil.copy(os.path.join(BRAND, "pcbcraft-icon-16.svg"), os.path.join(web, "favicon.svg"))

    def square(size, scale, pal=DARK, variant=FULL_COLOUR, tile=True):
        k = size / 512.0
        mark_px = 392 * k * scale
        off = (size - mark_px) / 2.0
        inner = ('<rect width="%d" height="%d" fill="%s"/>' % (size, size, TILE)) if tile else ""
        inner += '<g transform="translate(%s %s) scale(%s)">%s</g>' % (
            n(off), n(off), n(mark_px / 392.0), mark(pal, variant, "w"))
        return svg(size, size, "0 0 %d %d" % (size, size), inner, "PCBCraft app icon")

    for name, size, scale, tile, pal, variant in (
            ("apple-touch-icon", 180, 0.84, True, DARK, FULL_COLOUR),
            ("icon-maskable-512", 512, 0.72, True, DARK, FULL_COLOUR),
            ("icon-monochrome-512", 512, 0.72, False, MONO, FULL_MONO)):
        s = os.path.join(work, name + ".svg")
        write(s, square(size, scale, pal, variant, tile))
        render(s, os.path.join(web, name + ".png"), size)
    render(os.path.join(BRAND, "pcbcraft-icon.svg"), os.path.join(web, "icon-192.png"), 192)
    render(os.path.join(BRAND, "pcbcraft-icon.svg"), os.path.join(web, "icon-512.png"), 512)
    manifest = {
        "name": "PCBCraft",
        "short_name": "PCBCraft",
        "description": "Schematic capture, PCB layout and design checks in the browser.",
        "start_url": ".",
        "display": "standalone",
        "theme_color": TILE,
        "background_color": TILE,
        "icons": [
            {"src": "icon-192.png", "sizes": "192x192", "type": "image/png", "purpose": "any"},
            {"src": "icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any"},
            {"src": "icon-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable"},
            {"src": "icon-monochrome-512.png", "sizes": "512x512", "type": "image/png", "purpose": "monochrome"},
        ],
    }
    write(os.path.join(web, "manifest.webmanifest"), json.dumps(manifest, indent=2) + "\n")

    # Previews: every master, and the sized results the report lists.
    prev = os.path.join(GEN, "preview")
    for name in sorted(m):
        if name.startswith("pcbcraft-icon-"):
            continue
        base = name[:-4]
        if "logo" in name or "wordmark" in name:
            render_w(os.path.join(BRAND, name), os.path.join(prev, base + ".png"), 1200)
        else:
            render(os.path.join(BRAND, name), os.path.join(prev, base + ".png"), 512)
    for s in (16, 32, 128, 512):
        shutil.copy(os.path.join(lin, "%dx%d" % (s, s), "apps", APP_ID + ".png"),
                    os.path.join(prev, "icon-%d.png" % s))
    try:
        from PIL import Image
        for s, z in ((16, 16), (32, 8)):
            im = Image.open(os.path.join(prev, "icon-%d.png" % s)).convert("RGBA")
            im.resize((s * z, s * z), Image.NEAREST).save(os.path.join(prev, "zoom-%d.png" % s))
        sheet = Image.new("RGBA", (16 + 24 + 32 + 48 + 64 + 40, 80), (244, 242, 236, 255))
        x = 8
        for s in (16, 24, 32, 48, 64):
            im = Image.open(os.path.join(lin, "%dx%d" % (s, s), "apps", APP_ID + ".png")).convert("RGBA")
            sheet.alpha_composite(im, (x, 8))
            x += s + 8
        sheet.resize((sheet.width * 4, sheet.height * 4), Image.NEAREST).save(os.path.join(prev, "sizes-sheet.png"))
    except ImportError:
        print("Pillow missing: zoom previews skipped", file=sys.stderr)
    shutil.rmtree(work)
    print("brand build done")


if __name__ == "__main__":
    main()
