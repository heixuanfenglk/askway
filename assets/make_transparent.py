"""Clear icon background to true transparency and rebuild .ico / icon_256.png.

Windows often ignores alpha and paints RGB — so transparent pixels must be (0,0,0,0),
not (255,255,255,0). Also strip anti-aliased white fringe around the rounded tile.
"""
from __future__ import annotations

from collections import deque
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

root = Path(__file__).resolve().parent
src = root / "icon.png"
img = Image.open(src).convert("RGBA")
w, h = img.size
px = img.load()


def is_paper(r: int, g: int, b: int, a: int) -> bool:
    """White / near-white paper (incl. soft fringe), or already transparent."""
    if a < 8:
        return True
    # Broaden past pure white so AA fringe is cleared
    return r >= 220 and g >= 220 and b >= 220 and min(r, g, b) >= 200


visited = [[False] * w for _ in range(h)]
q: deque[tuple[int, int]] = deque()

seeds: list[tuple[int, int]] = [
    (0, 0),
    (w - 1, 0),
    (0, h - 1),
    (w - 1, h - 1),
]
step = max(1, min(w, h) // 128)
for x in range(0, w, step):
    seeds.append((x, 0))
    seeds.append((x, h - 1))
for y in range(0, h, step):
    seeds.append((0, y))
    seeds.append((w - 1, y))

for x, y in seeds:
    if is_paper(*px[x, y]):
        q.append((x, y))
        visited[y][x] = True

while q:
    x, y = q.popleft()
    if not is_paper(*px[x, y]):
        continue
    # Critical: zero RGB so Windows shell / taskbar won't paint a white square
    px[x, y] = (0, 0, 0, 0)
    for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
        if 0 <= nx < w and 0 <= ny < h and not visited[ny][nx]:
            visited[ny][nx] = True
            if is_paper(*px[nx, ny]):
                q.append((nx, ny))

# Second pass: any remaining transparent-but-white → black transparent
for y in range(h):
    for x in range(w):
        r, g, b, a = px[x, y]
        if a < 8:
            px[x, y] = (0, 0, 0, 0)

# Fit a rounded-rect mask to opaque content so corners stay clean after resize
xs, ys = [], []
for y in range(h):
    for x in range(w):
        if px[x, y][3] > 16:
            xs.append(x)
            ys.append(y)
minx, maxx, miny, maxy = min(xs), max(xs), min(ys), max(ys)
# pad slightly inward so residual fringe is clipped
pad = max(2, min(w, h) // 256)
minx, miny = minx + pad, miny + pad
maxx, maxy = maxx - pad, maxy - pad
side = max(maxx - minx + 1, maxy - miny + 1)
cx = (minx + maxx) / 2.0
cy = (miny + maxy) / 2.0
half = side / 2.0
left = int(max(0, cx - half))
top = int(max(0, cy - half))
right = int(min(w, cx + half))
bottom = int(min(h, cy + half))

cropped = img.crop((left, top, right, bottom)).resize((1024, 1024), Image.Resampling.LANCZOS)
radius = int(1024 * 0.22)
mask = Image.new("L", (1024, 1024), 0)
ImageDraw.Draw(mask).rounded_rectangle([1, 1, 1022, 1022], radius=radius, fill=255)
mask = mask.filter(ImageFilter.GaussianBlur(radius=0.8))

out = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
cp = cropped.load()
mp = mask.load()
op = out.load()
for y in range(1024):
    for x in range(1024):
        r, g, b, a = cp[x, y]
        m = mp[x, y]
        na = int(a * m / 255)
        if na < 10:
            op[x, y] = (0, 0, 0, 0)
            continue
        # Kill whitish AA on the soft mask edge (outside speech-bubble interior)
        if r > 230 and g > 230 and b > 230 and m < 252:
            op[x, y] = (0, 0, 0, 0)
            continue
        op[x, y] = (r, g, b, na)

out.save(src)
print(f"updated {src}")

sizes = [16, 24, 32, 48, 64, 128, 256]
frames = [out.resize((s, s), Image.Resampling.LANCZOS) for s in sizes]
# Zero RGB wherever alpha is low after downscale (LANCZOS bleed)
for fr in frames:
    fp = fr.load()
    sw, sh = fr.size
    for y in range(sh):
        for x in range(sw):
            r, g, b, a = fp[x, y]
            if a < 12:
                fp[x, y] = (0, 0, 0, 0)
            elif a < 200 and r > 220 and g > 220 and b > 220:
                # fringe: drop rather than show white halo
                fp[x, y] = (0, 0, 0, 0)

frames[-1].save(root / "icon_256.png")
frames[-1].save(
    root / "icon.ico",
    format="ICO",
    sizes=[(s, s) for s in sizes],
    append_images=frames[:-1],
)
print("rebuilt icon.ico and icon_256.png")

# verify
v = Image.open(root / "icon_256.png").convert("RGBA")
print("corners", [v.getpixel(p) for p in ((0, 0), (255, 0), (0, 255), (255, 255))])
print("edge mid", v.getpixel((128, 0)), v.getpixel((0, 128)))
diag = next((i, v.getpixel((i, i))) for i in range(64) if v.getpixel((i, i))[3] > 200)
print("first solid on diagonal", diag)
