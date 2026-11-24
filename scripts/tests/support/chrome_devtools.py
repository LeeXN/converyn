"""Launch an isolated Chrome and wait for its own loopback DevTools endpoint."""

from contextlib import contextmanager
import json
from os import SEEK_END
from pathlib import Path
import subprocess
import tempfile
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import ProxyHandler, build_opener


def _wait_for_devtools(child, profile, timeout):
    deadline = time.monotonic() + timeout
    port_file = profile / "DevToolsActivePort"
    opener = build_opener(ProxyHandler({}))
    while True:
        status = child.poll()
        if status is not None:
            raise RuntimeError(f"Chrome exited before DevTools was ready (status {status})")
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise RuntimeError(f"Chrome DevTools did not become ready within {timeout:g}s")
        try:
            port_text, browser_path = port_file.read_text(encoding="utf-8").splitlines()[:2]
            port = int(port_text)
            if 0 < port <= 65535 and browser_path.startswith("/devtools/browser/"):
                url = f"http://127.0.0.1:{port}/json/version"
                with opener.open(url, timeout=min(1, remaining)) as response:
                    endpoint = urlsplit(json.load(response)["webSocketDebuggerUrl"])
                if endpoint.path == browser_path and child.poll() is None:
                    return port
        except HTTPError as error:
            error.close()
        except (OSError, ValueError, KeyError, TypeError):
            # Chrome can create the file before the endpoint accepts a request.
            pass
        time.sleep(min(0.1, max(0, deadline - time.monotonic())))


def _stop(child):
    if child is None or child.poll() is not None:
        return
    child.terminate()
    try:
        child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()


@contextmanager
def chrome_devtools(chrome, workspace, report, timeout=30):
    report = report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="chrome-", dir=workspace, ignore_cleanup_errors=True
    ) as directory:
        profile = Path(directory)
        with tempfile.NamedTemporaryFile(
            prefix=f"{report.stem}.chrome-startup-", suffix=".log", dir=report.parent, delete=False
        ) as log:
            child = None
            try:
                try:
                    child = subprocess.Popen(
                        [str(chrome), "--headless=new", "--no-sandbox", "--disable-gpu",
                         "--disable-dev-shm-usage", "--remote-allow-origins=*",
                         "--remote-debugging-port=0", f"--user-data-dir={profile}"],
                        stdout=log, stderr=subprocess.STDOUT,
                    )
                    port = _wait_for_devtools(child, profile, timeout)
                except (OSError, RuntimeError) as error:
                    _stop(child)
                    log.seek(0, SEEK_END)
                    log.seek(max(0, log.tell() - 8192))
                    tail = log.read().decode("utf-8", errors="replace").strip()
                    raise RuntimeError(
                        f"{error}\nChrome startup log ({log.name}):\n{tail or '(no browser output)'}"
                    ) from error
                Path(log.name).unlink()
                yield port
            finally:
                _stop(child)
