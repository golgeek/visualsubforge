#!/usr/bin/env bash
set -euo pipefail
test "${GITHUB_ACTIONS:-}" = true
dir="$RUNNER_TEMP/review"
mkdir -p "$dir/VisualSubForge" "$dir/evidence"
app="$RUNNER_TEMP/input/VisualSubForge.app"
python="$RUNNER_TEMP/dmg-venv/bin/python"
swiftc review/finder-id.swift -o "$dir/finder-id"
sw_vers > "$dir/evidence/macos.txt"
function capture {
  local image=$1 name=$2 id
  hdiutil attach -readonly -noautoopen -nobrowse -mountpoint "$dir/VisualSubForge" "$image"
  trap 'hdiutil detach "$dir/VisualSubForge"' EXIT
  open "$dir/VisualSubForge"
  for _ in {1..25}; do
    if id=$("$dir/finder-id"); then break; fi
    sleep 1
  done
  if [[ -z "$id" ]]; then
    id=$(osascript -e 'with timeout of 10 seconds' -e 'tell application "Finder" to get id of first Finder window whose name is "VisualSubForge"' -e 'end timeout')
  fi
  sleep 2
  screencapture -x -o -l "$id" "$dir/evidence/$name.png"
  hdiutil detach "$dir/VisualSubForge"
  trap - EXIT
}
function appearance {
  local desired=false
  if [[ "$1" == dark ]]; then desired=true; fi
  osascript -e 'with timeout of 10 seconds' -e "tell application \"System Events\" to tell appearance preferences to set dark mode to $desired" -e 'end timeout'
  test "$(osascript -e 'tell application "System Events" to tell appearance preferences to get dark mode')" = "$desired"
  sleep 2
}
appearance light
"$python" scripts/macos_dmg.py build "$app" "$dir/trial.dmg"
capture "$dir/trial.dmg" trial
bar=$(python review/measure.py "$dir/evidence/trial.png")
[[ "$bar" =~ ^[0-9]+$ && "$bar" -lt 60 ]]
echo "Title bar: $bar pt" | tee -a "$dir/evidence/macos.txt"
sed -i '' "s/(640, 400)/(640, $((400+bar)))/" src-tauri/dmg/settings.py
for variant in chips no-chips; do
  if [[ "$variant" == no-chips ]]; then cp review/no-chips/*.png src-tauri/dmg/; fi
  "$python" scripts/macos_dmg.py build "$app" "$dir/$variant.dmg"
  "$python" scripts/macos_dmg.py verify "$app" "$dir/$variant.dmg"
  for mode in light dark; do
    appearance "$mode"
    capture "$dir/$variant.dmg" "$variant-$mode"
  done
done
