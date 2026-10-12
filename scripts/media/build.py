import os, subprocess, sys
# build.py stills_dir frames_dir out_dir — GIFs from the screencast frames, optimized stills, the cover.
stills, frames, out = sys.argv[1:4]
here = os.path.dirname(os.path.abspath(__file__))
OVERLAY = '720:640:280:40'  # the card of a 1280x720 overlay
GIFS = [
    ('timer', 900, 0, ''), ('tour', 900, 0, ''), ('theme', 900, 0, ''),
    ('alarm', 560, 0.25, OVERLAY), ('block', 560, 0.25, OVERLAY), ('wake', 560, 0.25, OVERLAY),
    ('nope', 360, 0.25, '400:360:440:191'),
]
for name, width, trim, crop in GIFS:
    subprocess.run([sys.executable, os.path.join(here, 'gif.py'), os.path.join(frames, name), os.path.join(out, name + '.gif'), str(width), '20', str(trim), crop], check=True)
subprocess.run([sys.executable, os.path.join(here, 'stills.py'), stills, out], check=True)
