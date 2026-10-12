#!/usr/bin/env python3
"""Attempt one bounded TCP connection for the hosted firewall positive/negative control."""
import argparse
import json
import socket

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("address", help="Previously resolved IPv4 address; no DNS during the control")
args = parser.parse_args()
try:
    with socket.create_connection((args.address, 443), timeout=3):
        result = {"connected": True}
except OSError as error:
    result = {"connected": False, "error": str(error), "errno": error.errno}
print(json.dumps(result))
