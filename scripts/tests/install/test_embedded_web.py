"""Verify the built binary serves its embedded web assets from an empty working directory."""
import hashlib,json,os,socket,ssl,subprocess,tempfile,time,unittest,urllib.error,urllib.request
from pathlib import Path
from test_installer import ROOT

@unittest.skipUnless(os.environ.get('CONVERYN_INSTALL_LIVE')=='1','explicit isolated HTTPS fixture')
class EmbeddedConsole(unittest.TestCase):
    def test_html_javascript_and_css_are_served_from_binary(self):
        binary=Path(os.environ.get('CONVERYN_INSTALL_BINARY',ROOT/'target/debug/converyn')).resolve()
        with tempfile.TemporaryDirectory(prefix='converyn-embedded-ui-') as directory:
            root=Path(directory);bundle=root/'bundle'
            with socket.socket() as listener:
                listener.bind(('127.0.0.1',0));port=listener.getsockname()[1]
            subprocess.run([str(binary),'admin','init','--directory',str(bundle),'--cluster','web-proof','--member',f'1=127.0.0.1:{port}'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,timeout=30)
            # The runtime starts in a directory with no web tree or repository files.
            self.assertFalse((root/'web').exists())
            with (root/'server.log').open('wb') as log:
                proc=subprocess.Popen([str(binary),'server','--config',str(bundle/'controller-1/server.json')],cwd=root,stdout=log,stderr=log)
                try:
                    context=ssl.create_default_context(cafile=str(bundle/'admin/ca.crt'))
                    context.load_cert_chain(str(bundle/'admin/tls.crt'),str(bundle/'admin/tls.key'))
                    http=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=context))
                    def fetch(path):
                        with http.open(f'https://127.0.0.1:{port}'+path,timeout=5) as response:
                            self.assertEqual(response.status,200);return response.read()
                    deadline=time.monotonic()+35;last=None
                    while time.monotonic()<deadline:
                        try:html=fetch('/');break
                        except (OSError,urllib.error.URLError) as error:last=error;time.sleep(.2)
                    else:self.fail('Server did not start: '+str(last))
                    js=fetch('/assets/app.js');css=fetch('/assets/index.css');version=json.loads(fetch('/assets/version.json'))['version']
                    self.assertEqual(js,(ROOT/'web/dist/assets/app.js').read_bytes())
                    self.assertEqual(css,(ROOT/'web/dist/assets/index.css').read_bytes())
                    self.assertEqual(version,hashlib.sha256(js+css).hexdigest())
                    self.assertIn(('/assets/app.js?v='+version).encode(),html)
                    self.assertIn(('/assets/index.css?v='+version).encode(),html)
                    self.assertFalse((root/'web').exists())
                finally:
                    proc.terminate()
                    try:proc.wait(timeout=10)
                    except subprocess.TimeoutExpired:proc.kill();proc.wait()

if __name__=='__main__':unittest.main()
