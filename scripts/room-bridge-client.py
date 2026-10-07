#!/usr/bin/env python3
"""Call the private room bridge from an authorized local client, without logging credentials."""
import argparse
import json
import os
from pathlib import Path
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tool", choices=["ocv.list_agents", "ocv.send_message", "ocv.get_message", "ocv.read_replies", "ocv.resume_room", "ocv.wake_agent"])
    parser.add_argument("--arguments-file", type=Path, help="JSON arguments file; defaults to an empty object")
    parser.add_argument("--client", type=Path, default=Path.home() / ".opencovibe-local/bridge-local-client.json")
    args = parser.parse_args()
    metadata = args.client.stat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise ValueError("Client configuration must be private to its owner")
    client = json.loads(args.client.read_text())
    url = urllib.parse.urlsplit(client["endpoint"])
    if url.scheme != "http" or url.hostname not in ("127.0.0.1", "::1") or url.path != "/mcp/ocv" or url.username or url.password or url.query or url.fragment:
        raise ValueError("Client endpoint must be the local room bridge")
    arguments = json.loads(args.arguments_file.read_text()) if args.arguments_file else {}
    if args.tool in ("ocv.send_message", "ocv.read_replies", "ocv.resume_room", "ocv.wake_agent"):
        arguments.setdefault("conversation_ref", client["conversation_ref"])
    request = urllib.request.Request(client["endpoint"], data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": args.tool, "arguments": arguments}}).encode(), headers={"Content-Type": "application/json", "Authorization": "Bearer " + client["bearer"]})
    # No proxy/redirect path can forward the local credential elsewhere.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *unused):
            raise ValueError("Bridge redirects are forbidden")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    try:
        with opener.open(request, timeout=20) as response:
            result = json.load(response)
    except urllib.error.HTTPError as error:
        raise ValueError(f"Bridge HTTP error: {error.code}") from None
    if "error" in result:
        raise ValueError(result["error"]["message"])
    print(json.dumps(result["result"]["structuredContent"], indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        print(f"Room bridge: {error}", file=sys.stderr)
        sys.exit(1)
