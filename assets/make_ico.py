from PIL import Image
from pathlib import Path

src = Path(__file__).with_name("icon.png")
dst = Path(__file__).with_name("icon.ico")
img = Image.open(src).convert("RGBA")

w, h = img.size
side = min(w, h)
left = (w - side) // 2
top = (h - side) // 2
img = img.crop((left, top, left + side, top + side))

sizes = [16, 24, 32, 48, 64, 128, 256]
# ICO writer expects the largest image as base; provide all sizes via sizes=
base = img.resize((256, 256), Image.Resampling.LANCZOS)
base.save(
    dst,
    format="ICO",
    sizes=[(s, s) for s in sizes],
)
print(f"wrote {dst} ({dst.stat().st_size} bytes)")
img.resize((256, 256), Image.Resampling.LANCZOS).save(
    Path(__file__).with_name("icon_256.png")
)
print("ok")
