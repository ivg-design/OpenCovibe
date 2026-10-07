#!/usr/bin/env python3
"""Outbound private room relay. Credentials and uncertain executions stay on this Mac."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request


def private_json(path):
    metadata = path.stat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise ValueError("Configuration must be private to its owner")
    return json.loads(path.read_text())


def save_private(path, value):
    temporary = path.with_suffix(".tmp")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump(value, stream)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *unused):
        raise ValueError("Redirects are forbidden")


def request_json(url, bearer, payload=None):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    request = urllib.request.Request(url, data=None if payload is None else json.dumps(payload).encode(),
                                     headers={"Authorization": "Bearer " + bearer, "Content-Type": "application/json", "User-Agent": "OpenCovibe-Room-Relay/0.3.6"})
    try:
        with opener.open(request, timeout=25) as response:
            body = response.read(2 * 1024 * 1024 + 1)
            if len(body) > 2 * 1024 * 1024:
                raise ValueError("Response exceeds limit")
            return json.loads(body)
    except urllib.error.HTTPError as error:
        raise ValueError(f"HTTP {error.code}") from None


def validate_configuration(config, client):
    remote = urllib.parse.urlsplit(config["relay_url"])
    if remote.scheme != "https" or not remote.hostname or remote.hostname in ("localhost", "127.0.0.1", "::1") or remote.username or remote.password or remote.query or remote.fragment or remote.path not in ("", "/"):
        raise ValueError("Relay must be an exact HTTPS origin")
    local = urllib.parse.urlsplit(client["endpoint"])
    if local.scheme != "http" or local.hostname not in ("127.0.0.1", "::1") or local.path != "/mcp/ocv" or local.username or local.password or local.query or local.fragment:
        raise ValueError("Native endpoint must be the loopback room bridge")
    if len(config["mac_token"]) < 40 or len(client["bearer"]) < 40:
        raise ValueError("Invalid private credential")


class Relay:
    def __init__(self, config, client, journal, transport=request_json):
        validate_configuration(config, client)
        self.config, self.client, self.journal, self.transport = config, client, journal, transport

    def remote(self, route, payload=None):
        return self.transport(self.config["relay_url"].rstrip("/") + route, self.config["mac_token"], payload)

    def acknowledge(self, record):
        result = self.remote("/bridge/respond", {"key": record["key"], "response": record["response"]})
        if result.get("ok") is not True:
            raise ValueError("Relay did not acknowledge response")
        save_private(self.journal, {"state": "settled"})

    def step(self):
        record = private_json(self.journal) if self.journal.exists() else {"state": "settled"}
        if record["state"] == "executing":
            # The process could have died after the native actor accepted the request.
            # Never execute it again, even if the remote client retries.
            record["response"] = {"jsonrpc": "2.0", "id": record["rpc_id"], "error": {
                "code": -32070, "message": "Native execution outcome is uncertain; inspect the message receipt before retrying",
                "data": {"request_key": record["key"], "state": "ambiguous"}}}
            record["state"] = "responding"
            save_private(self.journal, record)
        if record["state"] == "responding":
            self.acknowledge(record)
            return
        item = self.remote("/bridge/next").get("request")
        if item is None:
            return
        if not isinstance(item, dict) or not isinstance(item.get("key"), str) or not isinstance(item.get("rpc"), dict) or "id" not in item["rpc"]:
            raise ValueError("Invalid leased request")
        record = {"state": "executing", "key": item["key"], "rpc_id": item["rpc"]["id"]}
        save_private(self.journal, record)
        try:
            response = self.transport(self.client["endpoint"], self.client["bearer"], item["rpc"])
            if response.get("id") != record["rpc_id"] or response.get("jsonrpc") != "2.0":
                raise ValueError("Invalid native response")
        except (OSError, ValueError):
            # A transport error cannot establish whether native execution happened.
            return
        record.update(state="responding", response=response)
        save_private(self.journal, record)
        self.acknowledge(record)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=Path.home() / ".opencovibe-local/bridge-relay-client.json")
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--pending", action="store_true", help="Show owner-only consent details locally")
    args = parser.parse_args()
    config = private_json(args.config)
    client = private_json(Path(config["native_client"]))
    journal = args.config.with_name("bridge-relay-journal.json")
    relay = Relay(config, client, journal)
    if args.pending:
        for pending in relay.remote("/bridge/pending").get("pending", []):
            safe = {k: ''.join(c for c in str(v) if c.isprintable()) for k, v in pending.items()}
            print(f"{safe['client_name']} requests room access\nRequest: {safe.get('request_reference', safe['id'][-8:].upper())}\nRedirect: {safe['redirect_uri']}\nScope: {safe['scope']}\nOne-time owner code: {safe['code']}\nExpires: {safe['expires_at']}")
        record = private_json(journal) if journal.exists() else {}
        unknown = [r for r in relay.remote("/bridge/leased").get("requests", []) if r.get("key") != record.get("key")]
        if unknown:
            print(f"{len(unknown)} interrupted relay requests need manual reconciliation; they will not be replayed")
        return
    if not args.once:
        parser.error("The reactive relay is owned by OpenCovibe. Open the app; the background polling service has been retired.")
    lock = os.open(args.config.with_name("bridge-relay.lock"), os.O_CREAT | os.O_RDWR, 0o600)
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    relay.step()



if __name__ == "__main__":
    main()
