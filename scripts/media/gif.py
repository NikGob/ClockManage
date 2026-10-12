import json, os, subprocess, sys
# gif.py frames_dir out.gif width fps [trim_start_s]
src, out, width, fps = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
trim = float(sys.argv[5]) if len(sys.argv) > 5 else 0.0
crop = sys.argv[6] if len(sys.argv) > 6 else ''  # w:h:x:y in source pixels
times = json.load(open(os.path.join(src, 'times.json')))
lst = os.path.join(src, 'list.txt')
with open(lst, 'w') as f:
    for i, t in enumerate(times):
        if t - times[0] < trim:
            continue
        d = (times[i + 1] - t) if i + 1 < len(times) else 1 / fps
        f.write(f"file 'f{i:04d}.jpg'\nduration {max(d, 0.001):.4f}\n")
    f.write(f"file 'f{len(times) - 1:04d}.jpg'\n")
filt = f"fps={fps},{'crop=' + crop + ',' if crop else ''}scale={width}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle"
subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', lst, '-filter_complex', filt, '-loop', '0', out], check=True)
print(out, os.path.getsize(out) // 1024, 'KB')
