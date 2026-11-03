#!/usr/bin/env python3
"""Publish a verified tag's packages, then its GitLab Release (safe to retry)."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request


def publish(tag, directory):
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise ValueError("Expected a vMAJOR.MINOR.PATCH tag")
    api = os.environ["CI_API_V4_URL"]
    project = os.environ["CI_PROJECT_ID"]
    token = os.environ["CI_JOB_TOKEN"]
    base = f"{api}/projects/{project}"

    def request(method, url, data=None):
        headers = {"JOB-TOKEN": token}
        if isinstance(data, dict):
            data = json.dumps(data).encode()
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        with urllib.request.urlopen(req, timeout=120) as response:
            body = response.read()
            return json.loads(body) if body else None

    packages = sorted(directory.glob("plasma-visual-screensaver_*_amd64.deb"))
    if len(packages) != 1:
        raise ValueError("Expected exactly one amd64 Debian package")
    package = packages[0]
    checksum = package.with_name(package.name + ".sha256")
    subprocess.run(["sha256sum", "--check", checksum.name], cwd=directory, check=True)
    links = []
    for path in (package, checksum):
        name = urllib.parse.quote(path.name, safe="")
        url = f"{base}/packages/generic/plasma-visual-screensaver/{tag}/{name}"
        # Reuse only byte-identical uploads after a partially successful run.
        try:
            req = urllib.request.Request(url, headers={"JOB-TOKEN": token})
            with urllib.request.urlopen(req, timeout=120) as response:
                existing = response.read()
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
            error.close()
            request("PUT", url, path.read_bytes())
        else:
            if existing != path.read_bytes():
                raise ValueError(f"Refusing to replace existing package: {path.name}")
        # A public project must also expose its package registry anonymously.
        with urllib.request.urlopen(url, timeout=120) as response:
            if response.read() != path.read_bytes():
                raise ValueError(f"Public download differs: {path.name}")
        links.append({"name": path.name, "url": url,
                      "direct_asset_path": "/" + path.name, "link_type": "package"})
    tags = subprocess.check_output(["git", "tag", "--merged", f"refs/tags/{tag}",
                                    "--sort=-version:refname", "--list", "v*"], text=True).splitlines()
    previous = next((item for item in tags if item != tag), None)
    revision = f"refs/tags/{previous}..refs/tags/{tag}" if previous else f"refs/tags/{tag}"
    notes = subprocess.check_output(["git", "log", "--format=- %s (%h)", revision], text=True)
    # Annotated tags use their tagger date; lightweight tags use the commit date.
    released_at = subprocess.check_output(
        ["git", "for-each-ref", "--format=%(creatordate:iso-strict)", f"refs/tags/{tag}"],
        text=True).strip()
    if not released_at:
        raise ValueError(f"Missing timestamp for tag: {tag}")
    payload = {"name": f"Plasma Visual Screensaver {tag[1:]}", "tag_name": tag,
               "released_at": released_at,
               "description": f"Kubuntu 26.04 (amd64). Install the .deb with apt.\n\n{notes}",
               "assets": {"links": links}}
    endpoint = f"{base}/releases/{urllib.parse.quote(tag, safe='')}"
    try:
        existing = request("GET", endpoint)
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        error.close()
        release = request("POST", f"{base}/releases", payload)
    else:
        if any(link["url"] not in {item["url"] for item in existing["assets"]["links"]}
               for link in links):
            raise ValueError("Existing release has different assets; inspect it before retrying")
        release = existing
    print(release["_links"]["self"])


if __name__ == "__main__":
    publish(sys.argv[1], Path(sys.argv[2]))
