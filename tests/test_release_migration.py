#!/usr/bin/env python3
"""Exercise publication/retries and baseline retrieval without live forge writes."""
import contextlib
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


publisher = load_script('gitlab-release')
baseline = load_script('download-release-baseline')


class MigrationTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.uploads = {}
        self.release = None
        self.releases = {}
        self.fail_upload = False
        self.public_downloads = True
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def respond(self, code, body=b''):
                if isinstance(body, dict):
                    body = json.dumps(body).encode()
                self.send_response(code)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                if '/packages/generic/' in self.path:
                    if not owner.public_downloads and not self.headers.get('JOB-TOKEN'):
                        self.respond(403)
                        return
                    if self.path in owner.uploads:
                        self.respond(200, owner.uploads[self.path])
                    else:
                        self.respond(404)
                elif self.path.endswith('/releases/permalink/latest') and owner.releases:
                    self.respond(200, max(owner.releases.values(), key=lambda item: item['released_at']))
                elif self.path.rsplit('/', 1)[-1] in owner.releases:
                    self.respond(200, owner.releases[self.path.rsplit('/', 1)[-1]])
                else:
                    self.respond(404)

            def do_PUT(self):
                if self.headers.get('JOB-TOKEN') != 'test-job-token':
                    self.respond(403)
                    return
                if owner.fail_upload:
                    self.respond(500)
                    return
                owner.uploads[self.path] = self.rfile.read(int(self.headers['Content-Length']))
                self.respond(201, {'message': '201 Created'})

            def do_POST(self):
                if self.headers.get('JOB-TOKEN') != 'test-job-token':
                    self.respond(403)
                    return
                payload = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                payload['_links'] = {'self': 'https://gitlab.com/test/project/-/releases/' + payload['tag_name']}
                owner.release = payload
                owner.releases[payload['tag_name']] = payload
                self.respond(201, payload)

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        self.addCleanup(server.server_close)
        self.addCleanup(worker.join)
        self.addCleanup(server.shutdown)
        self.api = f'http://127.0.0.1:{server.server_port}/api/v4'
        self.env = patch.dict(os.environ, {
            'CI_API_V4_URL': self.api, 'CI_PROJECT_ID': '123',
            'CI_JOB_TOKEN': 'test-job-token', 'RELEASE_FORGE': 'gitlab',
        })
        self.env.start()
        self.addCleanup(self.env.stop)
        tree = self.directory / 'package'
        (tree / 'DEBIAN').mkdir(parents=True)
        (tree / 'DEBIAN/control').write_text(
            'Package: plasma-visual-screensaver\nVersion: 1.0.0-1\nArchitecture: amd64\n'
            'Maintainer: Test <test@example.org>\nDescription: baseline fixture\n')
        binary = tree / 'usr/bin/plasma-visual-screensaver'
        binary.parent.mkdir(parents=True)
        binary.write_text('#!/bin/sh\nexit 0\n')
        binary.chmod(0o755)
        self.package = self.directory / 'plasma-visual-screensaver_1.0.0-1_amd64.deb'
        subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(tree), str(self.package)],
                       check=True, stdout=subprocess.DEVNULL)
        self.checksum = self.package.with_name(self.package.name + '.sha256')
        self.checksum.write_text(hashlib.sha256(self.package.read_bytes()).hexdigest() + '  ' + self.package.name + '\n')
        self.git = patch.object(publisher.subprocess, 'check_output', side_effect=['v1.0.0\n', '- Initial release (abc123)\n', '2025-01-02T03:04:05+00:00\n'])

    def publish(self):
        # Notes are deterministic; all uploads and release API calls use real HTTP.
        with self.git, contextlib.redirect_stdout(io.StringIO()):
            publisher.publish('v1.0.0', self.directory)

    def test_publish_and_anonymous_baseline_download(self):
        self.publish()
        self.assertEqual(len(self.uploads), 2)
        self.assertEqual(self.release['tag_name'], 'v1.0.0')
        target = self.directory / 'baseline'
        baseline.download_baseline(target)
        self.assertTrue(os.access(target / 'release-prefix/usr/bin/plasma-visual-screensaver', os.X_OK))
        self.assertIn('v1.0.0', (target / 'frame-timing/baseline.txt').read_text())
        # Reusing identical artifacts does not create duplicates.
        with patch.object(publisher.subprocess, 'check_output', side_effect=['v1.0.0\n', '- Initial release (abc123)\n', '2025-01-02T03:04:05+00:00\n']):
            publisher.publish('v1.0.0', self.directory)
        self.assertEqual(len(self.uploads), 2)

    def test_out_of_order_republication_preserves_latest(self):
        self.releases['v2.0.0'] = {'tag_name': 'v2.0.0', 'released_at': '2026-01-01T00:00:00+00:00'}
        self.publish()
        self.assertEqual(self.release['released_at'], '2025-01-02T03:04:05+00:00')
        with baseline.urllib.request.urlopen(self.api + '/projects/123/releases/permalink/latest') as response:
            self.assertEqual(json.load(response)['tag_name'], 'v2.0.0')

    def test_conflicting_package_is_never_replaced(self):
        self.uploads['/api/v4/projects/123/packages/generic/plasma-visual-screensaver/v1.0.0/' + self.package.name] = b'different'
        with self.assertRaisesRegex(ValueError, 'Refusing to replace'):
            self.publish()
        self.assertIsNone(self.release)
        self.assertEqual(list(self.uploads.values()), [b'different'])

    def test_upload_failure_never_publishes_release(self):
        self.fail_upload = True
        with self.assertRaises(baseline.urllib.error.HTTPError) as caught:
            self.publish()
        caught.exception.close()
        self.assertIsNone(self.release)

    def test_private_downloads_never_publish_release(self):
        self.public_downloads = False
        with self.assertRaises(baseline.urllib.error.HTTPError) as caught:
            self.publish()
        caught.exception.close()
        self.assertIsNone(self.release)

    def test_bad_checksum_never_uploads(self):
        self.checksum.write_text('0' * 64 + '  ' + self.package.name + '\n')
        with self.assertRaises(subprocess.CalledProcessError):
            self.publish()
        self.assertFalse(self.uploads)
        self.assertIsNone(self.release)

    def test_github_baseline_is_selectable(self):
        self.publish()
        asset = self.release['assets']['links'][0]
        response = {'tag_name': 'v1.0.0', 'assets': [
            {'name': asset['name'], 'browser_download_url': asset['url']} ]}
        real_open = baseline.urllib.request.urlopen

        def urlopen(url, *args, **kwargs):
            if url == 'https://api.github.com/repos/StantonMatt/plasma-wayland-screensaver/releases/latest':
                return io.BytesIO(json.dumps(response).encode())
            return real_open(url, *args, **kwargs)

        with patch.dict(os.environ, {'RELEASE_FORGE': 'github'}), patch.object(baseline.urllib.request, 'urlopen', side_effect=urlopen):
            target = self.directory / 'github'
            baseline.download_baseline(target)
        self.assertTrue((target / 'release-prefix/usr/bin/plasma-visual-screensaver').exists())

    def test_unavailable_baseline_retains_absolute_checks(self):
        target = self.directory / 'unavailable'
        partial = target / 'release-prefix/usr/bin/plasma-visual-screensaver'
        partial.parent.mkdir(parents=True)
        partial.write_text('stale')
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/download-release-baseline.py'), str(target)],
                                text=True, capture_output=True, check=True)
        self.assertIn('using absolute cadence checks', result.stdout)
        self.assertFalse(partial.exists())


if __name__ == '__main__':
    unittest.main()
