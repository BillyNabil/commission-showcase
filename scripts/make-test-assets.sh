#!/usr/bin/env bash
# Generates synthetic test media into ./test-assets (needs ffmpeg with libvpx/prores).
# Used by `cargo run --example e2e` in core/.
set -euo pipefail
cd "$(dirname "$0")/.." && mkdir -p test-assets && cd test-assets
F="-y -loglevel error"
ffmpeg $F -f lavfi -i "testsrc2=s=600x600:r=30:d=3" -f lavfi -i "color=black:s=600x600:r=30:d=3,format=gray,geq=lum='if(lt(hypot(X-300,Y-300),220+60*sin(T*3)),255,0)'" \
  -filter_complex "[0][1]alphamerge,format=yuva420p" -c:v libvpx-vp9 -pix_fmt yuva420p -b:v 1M -auto-alt-ref 0 alpha.webm
ffmpeg $F -f lavfi -i "testsrc=s=800x450:r=24:d=2" -f lavfi -i "color=black:s=800x450:r=24:d=2,format=gray,geq=lum='if(lt(abs(X-400)+abs(Y-225),200),255,0)'" \
  -filter_complex "[0][1]alphamerge,format=yuva444p10le" -c:v prores_ks -profile:v 4444 -pix_fmt yuva444p10le prores.mov
ffmpeg $F -f lavfi -i "testsrc2=s=500x700:r=30:d=1.5" -f lavfi -i "color=black:s=500x700:r=30:d=1.5,format=gray,geq=lum='if(lt(hypot(X-250,Y-350),230),255,0)'" -f lavfi -i "sine=f=660:d=1.5" \
  -filter_complex "[0][1]alphamerge,format=yuva420p[v]" -map "[v]" -map 2:a -c:v libvpx-vp9 -pix_fmt yuva420p -auto-alt-ref 0 -c:a libopus short.webm
ffmpeg $F -f lavfi -i "color=c=black:s=1920x1080" -vf "geq=r='207+48*(X/W)':g='200+20*(X/W)':b='255-55*(X/W)'" -frames:v 1 bg.png
ffmpeg $F -f lavfi -i "color=c=0xff66aa:s=1920x1080,format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(lt(min(min(X,W-1-X),min(Y,H-1-Y)),36),255,0)'" -frames:v 1 frame.png
ffmpeg $F -f lavfi -i "color=c=black:s=1100x220,format=rgba,geq=r=24:g=22:b=34:a=230" -frames:v 1 sticker.png
ffmpeg $F -f lavfi -i "sine=f=440:d=6,volume=0.3" -c:a libmp3lame music.mp3
echo "test-assets ready: $(pwd)"
