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

# 纯视频(无音轨), 1 秒: 验证无音轨时墙钟接管走时。
ffmpeg -y \
  -f lavfi -i "color=c=blue:s=160x120:r=25:d=1" \
  -c:v libx264 -pix_fmt yuv420p \
  "$DIR/sample_video_only.mp4"
echo "生成: $DIR/sample_video_only.mp4"

# 音频(0.3s)先于视频(1s)结束: 验证音频 EOF 后墙钟接管, 播放不冻结。
ffmpeg -y \
  -f lavfi -i "color=c=green:s=160x120:r=25:d=1" \
  -f lavfi -t 0.3 -i "anullsrc=channel_layout=stereo:sample_rate=44100" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac \
  "$DIR/sample_short_audio.mp4"
echo "生成: $DIR/sample_short_audio.mp4"

# 双关键帧(0s/1s), 2 秒: 验证关键帧吸附 seek 的方向性(快进只向前吸附, 不回退)。
ffmpeg -y \
  -f lavfi -i "color=c=purple:s=160x120:r=25:d=2" \
  -force_key_frames "0,1" \
  -c:v libx264 -pix_fmt yuv420p \
  "$DIR/sample_gop.mp4"
echo "生成: $DIR/sample_gop.mp4"
