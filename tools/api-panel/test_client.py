import json
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from client import Client, ApiError

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_GET(self):
        if self.path == '/redirect':
            self.send_response(302); self.send_header('Location', '/v1/health'); self.end_headers(); return
        valid = self.headers.get('Authorization') == 'Bearer token'
        self.send_response(200 if valid else 401); self.end_headers()
        self.wfile.write(json.dumps({'ok':True,'result':{'api_version':1}} if valid else {'ok':False,'error':{'code':'unauthorized','message':'bad token'}}).encode())
    def do_POST(self):
        payload=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.send_response(200);self.end_headers()
        self.wfile.write(json.dumps({'ok':True,'result':payload}).encode())

class ClientTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True);cls.thread.start()
        cls.url=f'http://127.0.0.1:{cls.server.server_port}'
    @classmethod
    def tearDownClass(cls): cls.server.shutdown();cls.server.server_close();cls.thread.join()
    def test_health(self): self.assertEqual(Client(self.url,'token').request('/v1/health')['api_version'],1)
    def test_unauthorized(self):
        with self.assertRaisesRegex(ApiError,'HTTP 401'): Client(self.url,'wrong').request('/v1/health')
    def test_rpc_serialization(self): self.assertEqual(Client(self.url,'token').rpc('set_channel_volume',volume=42),{'method':'set_channel_volume','params':{'volume':42}})
    def test_no_remote_or_credential_urls(self):
        for url in ('http://example.com:80','http://localhost:80','https://127.0.0.1:80','http://u:p@127.0.0.1:80','http://127.0.0.1:80/extra'):
            with self.assertRaises(ApiError): Client(url,'token')
    def test_no_redirects(self):
        with self.assertRaisesRegex(ApiError,'Redirects'): Client(self.url,'token').request('/redirect')

if __name__=='__main__': unittest.main()
