"""Read the current portable client profile without exposing private credentials."""
import json
import os
from pathlib import Path
import ssl
import tempfile


def load(path, context=None):
    path = Path(path)
    if path.stat().st_mode & 0o077:
        raise ValueError('client profile must be private')
    value = json.loads(path.read_text())
    if value.get('version') != 1 or 'contexts' not in value:
        raise ValueError('unsupported client profile')
    return value['contexts'][context or value['current_context']]


def tls(config):
    credentials = config['credentials']
    context = ssl.create_default_context(cadata=credentials['ca_pem'])
    # OpenSSL consumes the key during load_cert_chain; private temporary files
    # can be removed before any network request is made.
    with tempfile.TemporaryDirectory(prefix='converyn-client-') as directory:
        cert = Path(directory) / 'certificate.pem'
        key = Path(directory) / 'key.pem'
        for path, value in [(cert, credentials['certificate_pem']),
                            (key, credentials['private_key_pem'])]:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, 'w') as output:
                output.write(value)
        context.load_cert_chain(cert, key)
    return context
