import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("relay", Path(__file__).with_name("room-bridge-relay.py"))
relay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(relay)


class RelayTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.journal = Path(self.folder.name) / "journal.json"
        self.config = {"relay_url": "https://relay.example.com", "mac_token": "m" * 48}
        self.client = {"endpoint": "http://127.0.0.1:9476/mcp/ocv", "bearer": "n" * 48}
        self.rpc = {"jsonrpc": "2.0", "id": "test", "method": "ping"}

    def tearDown(self):
        self.folder.cleanup()

    def test_acknowledgement_retry_never_reexecutes_native(self):
        calls = []
        def transport(url, token, payload=None):
            calls.append((url, token, payload))
            if url.endswith("/next"):
                return {"request": {"key": "q_test", "rpc": self.rpc}}
            if url.endswith("/mcp/ocv"):
                return {"jsonrpc": "2.0", "id": "test", "result": {}}
            if len(calls) == 3:
                raise OSError("Lost acknowledgement")
            return {"ok": True}
        service = relay.Relay(self.config, self.client, self.journal, transport)
        with self.assertRaises(OSError):
            service.step()
        service.step()
        self.assertEqual(sum(c[0].endswith("/mcp/ocv") for c in calls), 1)
        self.assertEqual(calls[2][2], calls[3][2])
        self.assertEqual(calls[1][1], self.client["bearer"])
        self.assertEqual(calls[0][1], self.config["mac_token"])

    def test_interrupted_native_execution_becomes_ambiguous(self):
        relay.save_private(self.journal, {"state": "executing", "key": "q_test", "rpc_id": "test"})
        calls = []
        def transport(url, token, payload=None):
            calls.append((url, payload))
            return {"ok": True}
        relay.Relay(self.config, self.client, self.journal, transport).step()
        self.assertEqual(len(calls), 1)
        self.assertTrue(calls[0][0].endswith("/respond"))
        self.assertEqual(calls[0][1]["response"]["error"]["data"]["state"], "ambiguous")

    def test_rejects_remote_native_endpoint_and_plain_http_relay(self):
        with self.assertRaises(ValueError):
            relay.validate_configuration(self.config, {**self.client, "endpoint": "https://other.example/mcp/ocv"})
        with self.assertRaises(ValueError):
            relay.validate_configuration({**self.config, "relay_url": "http://relay.example.com"}, self.client)


if __name__ == "__main__":
    unittest.main()
