#!/usr/bin/env python3
"""Render local evidence only. Requires Pillow; zero-origin CFR25 review fixture."""
import argparse
import json
import subprocess
from pathlib import Path
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('media')
parser.add_argument('sample', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
media = args.media
sample = json.loads(args.sample.read_text())
out = args.output
out.mkdir(parents=True, exist_ok=True)
# Save only selected adjacent pairs, not a recompressed copy of the input video.
frames=sorted({f for r in sample for f in [r['frame']-1,r['frame']]})
def balanced(items):
 if len(items)==1: return items[0]
 m=len(items)//2
 return '('+balanced(items[:m])+'+'+balanced(items[m:])+')'
select=balanced([f'eq(n,{n})' for n in frames])
subprocess.run(['ffmpeg','-v','error','-nostdin','-threads','28','-i',media,'-an','-sn','-dn','-filter_threads','1','-vf',f"select='{select}',scale=320:180",'-fps_mode','passthrough',str(out/'review-%03d.png')],check=True)
from PIL import Image,ImageDraw
for page in range((len(sample)+11)//12):
 im=Image.new('RGB',(1280,6*206),'#eeeeee');draw=ImageDraw.Draw(im)
 for j,r in enumerate(sample[page*12:(page+1)*12]):
  x=(j%2)*640;y=(j//2)*206
  draw.text((x+4,y+3),f"{page*12+j+1}: {r['seconds']:.2f}s",fill='black')
  k=frames.index(r['frame']-1)+1
  im.paste(Image.open(out/f'review-{k:03d}.png'),(x,y+25));im.paste(Image.open(out/f'review-{k+1:03d}.png'),(x+320,y+25))
 im.save(out/f'review-sheet-{page+1}.jpg')
print(f'Created {len(sample)} sampled frame-pair reviews')
