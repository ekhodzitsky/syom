#!/bin/sh
# TASK-10 host-capability probe. Emits JSON on stdout.
# Run on the candidate host *before* any Apple AAC measurement:
#   sh lab/apple/probe.sh
# Exit 0 always; the JSON tells whether the cell is executable.

printf '{\n'
printf '  "probe": "task-10-apple-aac",\n'
printf '  "date_utc": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '  "uname": "%s",\n' "$(uname -srm 2>/dev/null || echo unknown)"

# macOS identity + build (backend lineage evidence)
sv=$(sw_vers 2>/dev/null | tr '\n' ';'); [ -n "$sv" ] || sv="absent"
printf '  "sw_vers": "%s",\n' "$sv"

# afconvert: Apple CLI frontend over AudioToolbox
printf '  "afconvert": "%s",\n' "$(command -v afconvert 2>/dev/null || echo absent)"

# AudioToolbox / CoreAudio frameworks (the encoder lives here)
at="absent"
[ -e /System/Library/Frameworks/AudioToolbox.framework ] && at="present"
printf '  "audio_toolbox_framework": "%s",\n' "$at"
ca="absent"
[ -e /System/Library/Frameworks/CoreAudio.framework ] && ca="present"
printf '  "core_audio_framework": "%s",\n' "$ca"

# qaac / refalac frontends (Windows CoreAudioToolbox; refalac under Wine)
first_of() { for c in "$@"; do p=$(command -v "$c" 2>/dev/null) && { printf '%s' "$p"; return; }; done; printf 'absent'; }
printf '  "qaac": "%s",\n' "$(first_of qaac qaac64)"
printf '  "refalac": "%s",\n' "$(first_of refalac refalac64)"
printf '  "wine": "%s",\n' "$(first_of wine wine64)"

# Java (irrelevant to Apple AAC; recorded so the gap list is factual)
printf '  "java": "%s",\n' "$(command -v java 2>/dev/null || echo absent)"

case "$(uname -s)" in
  Darwin)
    [ "$at" = present ] && executable=true || executable=false
    ;;
  *)
    executable=false
    ;;
esac
printf '  "apple_aac_executable": %s\n' "$executable"
printf '}\n'
exit 0
