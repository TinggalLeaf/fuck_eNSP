#!/usr/bin/env python3
"""Generate fuck_ensp icons (Windows ICO + macOS ICNS + PNG set) from one master.

Design: dark rounded square, white magnifier over an orange bug,
four log-level dots (red/yellow/green/blue) along the bottom.
Rendered at 4096px with supersampling, downscaled with Lanczos.
"""
import io
import os
import struct

from PIL import Image, ImageDraw, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "src-tauri", "icons")
os.makedirs(OUT, exist_ok=True)

SS = 4                 # supersample factor
SIZE = 1024            # design-space size
S = SIZE * SS          # render size


def px(v):
    return int(round(v * SS))


def vgrad(size, top, bottom):
    grad = Image.new("RGBA", (1, size))
    for y in range(size):
        t = y / (size - 1)
        grad.putpixel((0, y), tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)) + (255,))
    return grad.resize((size, size))


def capsule(draw_img, p1, p2, width, fill):
    d = ImageDraw.Draw(draw_img)
    d.line([p1, p2], fill=fill, width=width)
    r = width // 2
    for x, y in (p1, p2):
        d.ellipse([x - r, y - r, x + r, y + r], fill=fill)


def circle_mask(radius, blur=0):
    m = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(m)
    c = S // 2
    d.ellipse([c - radius, c - radius, c + radius, c + radius], fill=255)
    if blur:
        m = m.filter(ImageFilter.GaussianBlur(blur))
    return m


def render_master():
    # ---- background: rounded square with vertical gradient
    bg = vgrad(S, (32, 41, 58), (10, 16, 30))          # #20293a -> #0a101e
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=px(224), fill=255)
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.paste(bg, (0, 0), mask)

    # soft top sheen
    sheen = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(sheen).ellipse([px(-100), px(-420), px(1124), px(420)], fill=(255, 255, 255, 26))
    img.alpha_composite(sheen.filter(ImageFilter.GaussianBlur(px(80))))

    # inner edge stroke
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([px(10), px(10), px(1014), px(1014)], radius=px(216),
                        outline=(255, 255, 255, 26), width=px(6))

    cx, cy = px(512), px(446)      # lens center
    ring_r_out = px(300)
    ring_r_in = px(252)
    lens_r = px(246)

    # ---- magnifier drop shadow (ring + handle silhouette)
    sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(sh).ellipse([cx - ring_r_out, cy - ring_r_out + px(28),
                                cx + ring_r_out, cy + ring_r_out + px(28)], fill=(0, 0, 0, 120))
    capsule(sh, (cx + px(208), cy + px(236)), (cx + px(364), cy + px(392)), px(64), (0, 0, 0, 120))
    img.alpha_composite(sh.filter(ImageFilter.GaussianBlur(px(26))))

    # ---- handle (under the ring)
    handle = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    capsule(handle, (cx + px(208), cy + px(208)), (cx + px(364), cy + px(364)), px(60), (241, 245, 249, 255))
    capsule(handle, (cx + px(196), cy + px(196)), (cx + px(352), cy + px(352)), px(24), (255, 255, 255, 255))
    img.alpha_composite(handle)

    # ---- lens glass tint
    glass = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(glass).ellipse([cx - lens_r, cy - lens_r, cx + lens_r, cy + lens_r], fill=(255, 255, 255, 20))
    img.alpha_composite(glass)

    # ---- bug, clipped to the lens
    bug = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    b = ImageDraw.Draw(bug)
    ORANGE, DARK, LINE = (249, 115, 22, 255), (194, 65, 12, 255), (124, 45, 18, 255)

    # legs (behind body): 3 per side, with a slight knee
    for side in (-1, 1):
        for y0, dy in ((492, -26), (548, 0), (604, 30)):
            x0 = 512 + side * 88
            knee = (512 + side * 168, y0 + dy - 34)
            tip = (512 + side * 210, y0 + dy + 22)
            b.line([(px(x0), px(y0)), (px(knee[0]), px(knee[1]))], fill=DARK, width=px(26))
            b.line([(px(knee[0]), px(knee[1])), (px(tip[0]), px(tip[1]))], fill=DARK, width=px(22))
            kr = px(13)
            b.ellipse([px(knee[0]) - kr, px(knee[1]) - kr, px(knee[0]) + kr, px(knee[1]) + kr], fill=DARK)

    # body
    b.ellipse([px(402), px(450), px(622), px(646)], fill=ORANGE, outline=DARK, width=px(8))
    # center split
    b.line([(px(512), px(470)), (px(512), px(628))], fill=LINE, width=px(10))
    # wing-case highlights
    b.ellipse([px(444), px(486), px(492), px(570)], fill=(254, 215, 170, 120))
    b.ellipse([px(532), px(486), px(580), px(570)], fill=(254, 215, 170, 120))

    # head
    b.ellipse([px(450), px(340), px(574), px(464)], fill=ORANGE, outline=DARK, width=px(8))
    # eyes
    for ex in (486, 538):
        b.ellipse([px(ex - 12), px(386 - 12), px(ex + 12), px(386 + 12)], fill=(255, 247, 237, 255))
    # antennae
    for side in (-1, 1):
        b.line([(px(512 + side * 24), px(352)), (px(512 + side * 70), px(298))], fill=DARK, width=px(18))
        ar = px(11)
        ax, ay = px(512 + side * 70), px(298)
        b.ellipse([ax - ar, ay - ar, ax + ar, ay + ar], fill=DARK)

    # clip bug to lens circle (translate coords: bug drawn with lens center at (512,446) in design space)
    shift = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    shift.paste(bug, (cx - px(512), cy - px(446)))
    lensmask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(lensmask).ellipse([cx - lens_r, cy - lens_r, cx + lens_r, cy + lens_r], fill=255)
    img.paste(shift, (0, 0), lensmask)

    # ---- ring on top
    ring = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    rd = ImageDraw.Draw(ring)
    rd.ellipse([cx - ring_r_out, cy - ring_r_out, cx + ring_r_out, cy + ring_r_out], fill=(248, 250, 252, 255))
    rd.ellipse([cx - ring_r_in, cy - ring_r_in, cx + ring_r_in, cy + ring_r_in], fill=(0, 0, 0, 0))
    rd.ellipse([cx - ring_r_in, cy - ring_r_in, cx + ring_r_in, cy + ring_r_in],
               outline=(203, 213, 225, 255), width=px(6))
    img.alpha_composite(ring)

    # ---- log-level dots along the bottom
    dots = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    dd = ImageDraw.Draw(dots)
    colors = [(239, 68, 68), (250, 204, 21), (34, 197, 94), (59, 130, 246)]
    ys = px(908)
    for i, c in enumerate(colors):
        x = px(392 + i * 80)
        dd.ellipse([x - px(26), ys - px(26), x + px(26), ys + px(26)], fill=c + (255,))
    img.alpha_composite(dots.filter(ImageFilter.GaussianBlur(px(10))))
    img.alpha_composite(dots)

    return img


def write_ico(path, imgs):
    entries = []
    for size in sorted(imgs, reverse=True):
        buf = io.BytesIO()
        imgs[size].save(buf, "PNG", optimize=True)
        png = buf.getvalue()
        dim = 0 if size >= 256 else size
        entries.append((dim, png))
    header = struct.pack("<HHH", 0, 1, len(entries))
    offset = 6 + 16 * len(entries)
    directory, data = b"", b""
    for dim, png in entries:
        directory += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset)
        data += png
        offset += len(png)
    with open(path, "wb") as f:
        f.write(header + directory + data)


def write_icns(path, type_sizes, imgs):
    payload = b""
    for t, size in type_sizes.items():
        buf = io.BytesIO()
        imgs[size].save(buf, "PNG", optimize=True)
        png = buf.getvalue()
        payload += t.encode("ascii") + struct.pack(">I", 8 + len(png)) + png
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", 8 + len(payload)) + payload)


def main():
    master = render_master()
    sizes = [16, 20, 24, 30, 32, 40, 44, 48, 64, 71, 89, 107, 128, 142, 150, 256, 284, 310, 512, 1024]
    imgs = {n: master.resize((n, n), Image.LANCZOS) for n in sizes}

    imgs[1024].save(os.path.join(OUT, "icon.png"))
    for n in (32, 128):
        imgs[n].save(os.path.join(OUT, f"{n}x{n}.png"))
    imgs[256].save(os.path.join(OUT, "128x128@2x.png"))
    for n in (30, 44, 71, 89, 107, 142, 150, 284, 310):
        imgs[n].save(os.path.join(OUT, f"Square{n}x{n}Logo.png"))
    imgs[71].save(os.path.join(OUT, "StoreLogo.png"))

    write_ico(os.path.join(OUT, "icon.ico"), {n: imgs[n] for n in (16, 20, 24, 32, 40, 48, 64, 128, 256)})
    write_icns(os.path.join(OUT, "icon.icns"),
               {"ic12": 32, "ic11": 64, "ic07": 128, "ic08": 256, "ic09": 512, "ic10": 1024}, imgs)

    # preview strip for review (fixed grid so every tile is fully visible)
    preview_sizes = [16, 24, 32, 48, 64, 128, 256, 512]
    pad, cell = 24, 560
    prev = Image.new("RGBA", (pad * (len(preview_sizes) + 1) + cell * len(preview_sizes), pad * 2 + cell),
                     (58, 62, 70, 255))
    for i, n in enumerate(preview_sizes):
        tile = imgs[n]
        x = pad + i * (cell + pad)
        prev.paste(tile, (x + (cell - n) // 2, pad + (cell - n) // 2), tile)
    prev.save(os.path.join(ROOT, "icon_preview.png"))
    print("icons written to", OUT)
    print("preview ->", os.path.join(ROOT, "icon_preview.png"))


if __name__ == "__main__":
    main()
