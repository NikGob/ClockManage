import os, sys
from PIL import Image, ImageDraw, ImageFilter
# stills.py src_dir dst_dir — copy the stills (optimized) and build the light/dark cover.
src, dst = sys.argv[1], sys.argv[2]
names = ['today-light', 'plan', 'blocking', 'log', 'week', 'nap', 'mini-dark', 'mini-light']
for n in names:
    im = Image.open(os.path.join(src, n + '.png'))
    if not n.startswith('mini'):
        im = im.convert('RGB')
    im.save(os.path.join(dst, n + '.png'), optimize=True)

# Cover: dark on the left, light on the right, split along a diagonal; rounded, on a soft shadow.
dark = Image.open(os.path.join(src, 'today-dark.png')).convert('RGB')
light = Image.open(os.path.join(src, 'work-light.png')).convert('RGB')
w, h = dark.size
mask = Image.new('L', (w, h), 0)
ImageDraw.Draw(mask).polygon([(int(w * 0.60), 0), (w, 0), (w, h), (int(w * 0.44), h)], fill=255)
mix = Image.composite(light, dark, mask)
r = 28
round_mask = Image.new('L', (w, h), 0)
ImageDraw.Draw(round_mask).rounded_rectangle([0, 0, w - 1, h - 1], r, fill=255)
pad = 56
canvas = Image.new('RGBA', (w + pad * 2, h + pad * 2), (0, 0, 0, 0))
shadow = Image.new('RGBA', canvas.size, (0, 0, 0, 0))
ImageDraw.Draw(shadow).rounded_rectangle([pad, pad + 14, pad + w, pad + h + 14], r, fill=(0, 0, 0, 120))
shadow = shadow.filter(ImageFilter.GaussianBlur(22))
canvas.alpha_composite(shadow)
card = mix.convert('RGBA')
card.putalpha(round_mask)
canvas.alpha_composite(card, (pad, pad))
canvas.save(os.path.join(dst, 'cover.png'), optimize=True)
for n in sorted(os.listdir(dst)):
    print(n, os.path.getsize(os.path.join(dst, n)) // 1024, 'KB')
