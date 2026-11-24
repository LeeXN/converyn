import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "support"))
from chrome_devtools import chrome_devtools


class ChromeDevToolsTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.workspace = Path(directory.name)
        self.report = self.workspace / "reports" / "browser.json"

    def fake_chrome(self, source):
        chrome = self.workspace / "fake-chrome"
        chrome.write_text(f"#!{sys.executable}\n" + textwrap.dedent(source), encoding="utf-8")
        chrome.chmod(0o700)
        return chrome

    def startup_logs(self):
        return list(self.report.parent.glob("browser.chrome-startup-*.log"))

    def test_waits_for_chromes_assigned_port_and_ready_endpoint(self):
        chrome = self.fake_chrome('''
            import json
            import sys
            from http.server import BaseHTTPRequestHandler, HTTPServer
            from pathlib import Path

            assert "--remote-debugging-port=0" in sys.argv
            profile = Path(next(arg.split("=", 1)[1] for arg in sys.argv if arg.startswith("--user-data-dir=")))
            browser_path = "/devtools/browser/fixture"

            class Handler(BaseHTTPRequestHandler):
                requests = 0

                def do_GET(self):
                    assert self.path == "/json/version"
                    Handler.requests += 1
                    if Handler.requests == 1:
                        self.send_error(503)
                        return
                    (profile.parent / "ready-port").write_text(str(self.server.server_port))
                    self.send_response(200)
                    self.end_headers()
                    self.wfile.write(json.dumps({
                        "webSocketDebuggerUrl": f"ws://127.0.0.1:{self.server.server_port}{browser_path}"
                    }).encode())

            with HTTPServer(("127.0.0.1", 0), Handler) as server:
                (profile / "DevToolsActivePort").write_text(f"{server.server_port}\\n{browser_path}\\n")
                server.serve_forever()
        ''')
        with chrome_devtools(chrome, self.workspace, self.report, timeout=5) as port:
            self.assertEqual(port, int((self.workspace / "ready-port").read_text()))
        self.assertEqual(self.startup_logs(), [])

    def test_preserves_startup_stderr_if_chrome_exits(self):
        chrome = self.fake_chrome('''
            import sys
            print("startup failed in fixture", file=sys.stderr)
            sys.exit(17)
        ''')
        with self.assertRaisesRegex(RuntimeError, r"(?s)status 17.*startup failed in fixture"):
            with chrome_devtools(chrome, self.workspace, self.report, timeout=5):
                self.fail("browser fixture must not run after a startup failure")
        log, = self.startup_logs()
        self.assertIn("startup failed in fixture", log.read_text())
        self.assertEqual(log.stat().st_mode & 0o077, 0)

    def test_timeout_never_yields_a_browser_port(self):
        chrome = self.fake_chrome('''
            import time
            print("still starting", flush=True)
            time.sleep(60)
        ''')
        with self.assertRaisesRegex(RuntimeError, "Chrome DevTools did not become ready"):
            with chrome_devtools(chrome, self.workspace, self.report, timeout=1):
                self.fail("browser fixture must not run after a readiness timeout")
        self.assertEqual(len(self.startup_logs()), 1)
