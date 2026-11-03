#!/usr/bin/env python3
"""Fetch one immutable latest-release baseline; failures retain absolute checks."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.error
import urllib.request


def download_baseline(directory):
    forge = os.environ.get("RELEASE_FORGE", "gitlab")
    if forge == "gitlab":
        api = os.environ.get("CI_API_V4_URL", "https://gitlab.com/api/v4")
        project = os.environ.get("CI_PROJECT_ID", "88430057")
        endpoint = f"{api}/projects/{project}/releases/permalink/latest"
    elif forge == "github":
        repo = os.environ.get("GITHUB_REPOSITORY", "StantonMatt/plasma-wayland-screensaver")
        endpoint = f"https://api.github.com/repos/{repo}/releases/latest"
    else:
        raise ValueError(f"Unknown RELEASE_FORGE: {forge}")
    with urllib.request.urlopen(endpoint, timeout=20) as response:
        release = json.load(response)
    assets = release["assets"]["links"] if forge == "gitlab" else release["assets"]
    packages = [asset for asset in assets if re.fullmatch(
        r"plasma-visual-screensaver_[^/]+_amd64\.deb", asset["name"])]
    if len(packages) != 1:
        raise ValueError("Release package missing or ambiguous")
    package = packages[0]
    destination = directory / "release-package" / package["name"]
    destination.parent.mkdir(parents=True, exist_ok=True)
    url = package["url"] if forge == "gitlab" else package["browser_download_url"]
    # A hard wall-clock bound also covers a slow trickle of bytes.
    subprocess.run(["timeout", "60s", sys.executable, "-c",
                    "import sys, urllib.request; urllib.request.urlretrieve(sys.argv[1], sys.argv[2])",
                    url, str(destination)], check=True)
    prefix = directory / "release-prefix"
    subprocess.run(["dpkg-deb", "-x", str(destination), str(prefix)], check=True)
    if not os.access(prefix / "usr/bin/plasma-visual-screensaver", os.X_OK):
        raise ValueError("Release binary missing or not executable")
    identity = f"Release baseline: {release['tag_name']} ({package['name']})"
    (directory / "frame-timing").mkdir(parents=True, exist_ok=True)
    (directory / "frame-timing/baseline.txt").write_text(identity + "\n")
    print(identity)


if __name__ == "__main__":
    directory = Path(sys.argv[1])
    try:
        download_baseline(directory)
    except Exception as error:
        # Do not leave a partial extraction eligible for comparison.
        import shutil
        shutil.rmtree(directory / "release-prefix", ignore_errors=True)
        print(f"WARNING: Release baseline unavailable ({error}); using absolute cadence checks.")
        if isinstance(error, urllib.error.HTTPError):
            error.close()
