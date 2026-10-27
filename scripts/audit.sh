#!/bin/sh
set -eu
# SQLx's lockfile includes its optional MySQL RSA dependency. Never waive this
# advisory if that dependency becomes reachable in the selected build graph.
active_rsa="$(cargo tree --locked --edges normal,build --invert rsa)"
if [ -n "$active_rsa" ]; then
  printf '%s\n' 'RSA is active: remove the exception and review RUSTSEC-2023-0071.' >&2
  exit 1
fi
printf '%s\n' 'Lockfile-only exception: RUSTSEC-2023-0071 (rsa, unused MySQL driver).'
cargo audit --ignore RUSTSEC-2023-0071
