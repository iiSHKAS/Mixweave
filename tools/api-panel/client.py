"""Standard-library Mixweave local API client (usable without Qt)."""
import json
import os
from pathlib import Path
import urllib.error
import urllib.parse
import urllib.request


class ApiError(RuntimeError):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ApiError('Redirects are not allowed for authenticated local requests')


class Client:
    def __init__(self, url, token, timeout=15):
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme != 'http' or parsed.hostname != '127.0.0.1' or not parsed.port or parsed.username or parsed.password or parsed.query or parsed.fragment or parsed.path not in ('', '/'):
            raise ApiError('URL must be http://127.0.0.1:PORT')
        self.url, self.token, self.timeout = url.rstrip('/'), token, timeout
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())

    @staticmethod
    def discovery_path():
        return Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config')) / 'mixweave/local-api.json'

    @classmethod
    def discover(cls):
        data = json.loads(cls.discovery_path().read_text())
        return cls(data['url'], data['token'])

    def request(self, path, payload=None):
        data = None if payload is None else json.dumps(payload).encode()
        req = urllib.request.Request(self.url + path, data=data, headers={
            'Authorization': 'Bearer ' + self.token, 'Content-Type': 'application/json'})
        try:
            with self.opener.open(req, timeout=self.timeout) as response:
                value = json.load(response)
        except urllib.error.HTTPError as exc:
            body = exc.read().decode(errors='replace')
            try:
                error = json.loads(body)['error']
                body = f"{error['code']}: {error['message']}"
            except (ValueError, KeyError, TypeError):
                pass
            raise ApiError(f'HTTP {exc.code}: {body}') from exc
        except (OSError, urllib.error.URLError) as exc:
            raise ApiError(f'Connection failed: {exc}. Start Mixweave and rediscover the endpoint.') from exc
        if not value.get('ok'):
            raise ApiError(str(value))
        return value['result']

    def rpc(self, method, **params):
        return self.request('/v1/rpc', {'method': method, 'params': params})

    def snapshot(self):
        return self.rpc('get_profile_snapshot')
