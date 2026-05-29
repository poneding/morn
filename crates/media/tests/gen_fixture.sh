#!/usr/bin/env bash
# 生成一个 1 秒、160x120、25fps、纯色视频 + 静音 AAC 音轨的测试 mp4。
set -euo pipefail
DIR="$(dirname "$0")/fixtures"
mkdir -p "$DIR"
ffmpeg -y \
  -f lavfi -i "color=c=red:s=160x120:r=25:d=1" \
  -f lavfi -i "anullsrc=channel_layout=stereo:sample_rate=44100" \
  -shortest -c:v libx264 -pix_fmt yuv420p -c:a aac \
  "$DIR/sample.mp4"
echo "生成: $DIR/sample.mp4"
