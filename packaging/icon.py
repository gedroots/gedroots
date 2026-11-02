# Génère l'icône GEDRoots (1024 px) : arbre-pedigree bleu au-dessus, racines ambrées.
import sys
from PIL import Image, ImageDraw, ImageFilter
S = 4096  # suréchantillonnage
u = S / 1024
def P(x, y): return (x * u, y * u)
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
# Fond : carré arrondi façon macOS (marge 100 px sur 1024, rayon 185).
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([P(100, 100), P(924, 924)], radius=185 * u, fill=255)
grad = Image.new("RGBA", (S, S))
gd = ImageDraw.Draw(grad)
top, bot = (0x2a, 0x30, 0x3c), (0x12, 0x14, 0x18)
for y in range(S):
    t = y / S
    gd.line([(0, y), (S, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bot)) + (255,))
# Ombre portée légère
shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(shadow).rounded_rectangle([P(100, 112), P(924, 936)], radius=185 * u, fill=(0, 0, 0, 110))
shadow = shadow.filter(ImageFilter.GaussianBlur(14 * u))
img.alpha_composite(shadow)
img.paste(grad, (0, 0), mask)
d = ImageDraw.Draw(img)
blue, blue2 = (0x7a, 0xa9, 0xff, 255), (0xa9, 0xc6, 0xff, 255)
amber = (0xe8, 0xa3, 0x5d, 255)
W = 26 * u
def line(a, b, col, w=W):
    d.line([P(*a), P(*b)], fill=col, width=int(w))
    for p in (a, b):
        r = w / 2
        x, y = P(*p); d.ellipse([x - r, y - r, x + r, y + r], fill=col)
def node(p, r, col, ring=None):
    x, y = P(*p); r *= u
    d.ellipse([x - r, y - r, x + r, y + r], fill=col)
    if ring:
        rr = r * 0.45
        d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=ring)
# Racines (ambre) : courbes de Bézier effilées sous le tronc
def root(p0, p1, p2, w0, w1, col=amber, n=160):
    for i in range(n + 1):
        t = i / n
        x = (1-t)**2*p0[0] + 2*(1-t)*t*p1[0] + t*t*p2[0]
        y = (1-t)**2*p0[1] + 2*(1-t)*t*p1[1] + t*t*p2[1]
        r = (w0 + (w1 - w0) * t) / 2 * u
        X, Y = P(x, y); d.ellipse([X-r, Y-r, X+r, Y+r], fill=col)
base = (512, 610)
root(base, (500, 700), (300, 790), 34, 10)
root(base, (524, 700), (724, 790), 34, 10)
root(base, (480, 720), (420, 830), 28, 9)
root(base, (544, 720), (604, 830), 28, 9)
root((400, 744), (360, 740), (330, 715), 12, 6)
root((624, 744), (664, 740), (694, 715), 12, 6)
# Tronc + pedigree (bleu) : 1 → 2 → 4
root(base, (512, 540), (512, 470), 38, 30, blue)
g1 = [(370, 360), (654, 360)]
g2 = [(290, 230), (450, 230), (574, 230), (734, 230)]
for p in g1: line((512, 470), p, blue)
for i, p in enumerate(g2): line(g1[i // 2], p, blue, 22 * u)
node((512, 470), 50, blue2, (0x12, 0x14, 0x18, 255))
for p in g1: node(p, 40, blue2, (0x12, 0x14, 0x18, 255))
for p in g2: node(p, 32, blue2)
img.resize((1024, 1024), Image.LANCZOS).save(sys.argv[1])
