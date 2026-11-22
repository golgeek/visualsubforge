"""Locate the arrow's horizontal stem in a Finder capture, in window points."""
import struct
import subprocess
import sys
from pathlib import Path

png = Path(sys.argv[1])
bmp = png.with_suffix(".bmp")
subprocess.run(["sips", "-s", "format", "bmp", str(png), "--out", str(bmp)], check=True, stdout=subprocess.DEVNULL)
data = bmp.read_bytes()
bmp.unlink()
offset, = struct.unpack_from("<I", data, 10)
width, height = struct.unpack_from("<ii", data, 18)
bpp, = struct.unpack_from("<H", data, 28)
assert width % 640 == 0 and bpp in (24, 32), (width, height, bpp)
scale = width // 640
stride = (width * bpp + 31) // 32 * 4
def rgb(x, y):
    row = height - y - 1 if height > 0 else y
    start = offset + row * stride + x * (bpp // 8)
    b,g,r = data[start:start+3]
    return r,g,b
orange = []
for y in range(abs(height)):
    r,g,b = rgb(310*scale,y)
    if r > 220 and 65 < g < 165 and b < 115:
        orange.append(y)
assert 2*scale <= len(orange) <= 6*scale, orange
origin = round((min(orange) + max(orange) + 1) / (2*scale) - 186)
print(origin)
