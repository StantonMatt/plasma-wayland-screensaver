#!/usr/bin/env python3
"""Check the executable's interposer ABI, including after package stripping."""
import subprocess
import sys

symbols = {
    "wl_display_create_queue_with_name",
    "wl_display_dispatch_queue_pending",
    "wl_event_queue_destroy",
    "wl_display_disconnect",
}
output = subprocess.check_output(
    ["readelf", "--wide", "--dyn-syms", sys.argv[1]], text=True
)
exported = set()
for line in output.splitlines():
    fields = line.split()
    if (len(fields) >= 8 and fields[3:6] == ["FUNC", "GLOBAL", "DEFAULT"]
            and fields[6] != "UND"):
        exported.add(fields[7])
missing = symbols - exported
if missing:
    raise SystemExit("Missing EGL drain exports: " + ", ".join(sorted(missing)))
print(f"{sys.argv[1]}: all four EGL drain exports present")
