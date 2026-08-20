#!/bin/bash
set -u
cd /home/parobek/Code/OSS_Public-Projects/Rusty2600
BIN=target/release/examples/dump_frame
OUT=screenshots/homebrew
TMP=/tmp/claude-1000/-home-parobek-Code-OSS-Public-Projects-Rusty2600/e0ae5628-6703-4986-9085-2877728c0498/scratchpad/shots
mkdir -p "$TMP"
count=0
fail=0
for rom in tests/roms/homebrew/*.a26; do
  base=$(basename "$rom" .a26)
  count=$((count+1))
  rm -f "$TMP"/frame_*.ppm
  if timeout 20 "$BIN" "$rom" 300 "$TMP" 299 >/tmp/dumpframe.log 2>&1; then
    last=$(ls "$TMP"/frame_*.ppm 2>/dev/null | sort | tail -1)
    if [ -n "$last" ]; then
      magick "$last" "$OUT/$base.png" 2>/dev/null
      echo "[$count] OK: $base"
    else
      echo "[$count] NO FRAME: $base"
      fail=$((fail+1))
    fi
  else
    echo "[$count] TIMEOUT/CRASH: $base"
    fail=$((fail+1))
  fi
done
echo "DONE: $count processed, $fail failed"
