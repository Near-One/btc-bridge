#!/usr/bin/env python3
import base64
import json
import os
import urllib.request

TOKEN = "nzec.bridge.near"
HOLDERS_FILE = "./zcash_migration_data/nzec_holders.json"
RPC = "https://rpc.mainnet.near.org"
BALANCES_PREFIX = b"\x00"
DECIMALS = 8
PRICE_URL = "https://api.coingecko.com/api/v3/simple/price?ids=zcash&vs_currencies=usd"


def zec_price_usd():
    with urllib.request.urlopen(PRICE_URL) as resp:
        return json.load(resp)["zcash"]["usd"]


def fmt(balance, price):
    zec = balance / 10**DECIMALS
    return f"{balance:>12} {zec:>14.8f} {zec * price:>10.2f}"


def rpc(method, params):
    req = urllib.request.Request(
        RPC,
        data=json.dumps(
            {"jsonrpc": "2.0", "id": "1", "method": method, "params": params}
        ).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req) as resp:
        body = json.load(resp)
    if "error" in body or "error" in body.get("result", {}):
        raise RuntimeError(body.get("error") or body["result"]["error"])
    return body["result"]


def view_call(method, args):
    result = rpc(
        "query",
        {
            "request_type": "call_function",
            "finality": "final",
            "account_id": TOKEN,
            "method_name": method,
            "args_base64": base64.b64encode(json.dumps(args).encode()).decode(),
        },
    )
    return json.loads(bytes(result["result"]))


def main():
    state = rpc(
        "query",
        {
            "request_type": "view_state",
            "finality": "final",
            "account_id": TOKEN,
            "prefix_base64": base64.b64encode(BALANCES_PREFIX).decode(),
        },
    )

    holders = []
    for item in state["values"]:
        key = base64.b64decode(item["key"])
        raw = key[len(BALANCES_PREFIX) :]
        str_len = int.from_bytes(raw[:4], "little")
        account = raw[4 : 4 + str_len].decode()
        balance = int.from_bytes(base64.b64decode(item["value"]), "little")
        holders.append((account, balance))

    holders.sort(key=lambda h: -h[1])
    nonzero = [h for h in holders if h[1] > 0]

    os.makedirs(os.path.dirname(HOLDERS_FILE), exist_ok=True)
    with open(HOLDERS_FILE, "w") as f:
        json.dump([account for account, _ in nonzero], f)
    print(f"Saved {len(nonzero)} holders to {HOLDERS_FILE}")

    price = zec_price_usd()

    print(f"ZEC price: ${price}")
    print(f"{'ACCOUNT':<70} {'BALANCE':>12} {'ZEC':>14} {'USD':>10}")
    for account, balance in nonzero:
        print(f"{account:<70} {fmt(balance, price)}")

    total_sum = sum(b for _, b in holders)
    total_supply = int(view_call("ft_total_supply", {}))

    print()
    print(f"Registered accounts: {len(holders)}, non-zero: {len(nonzero)}")
    print(f"Sum of balances: {fmt(total_sum, price)}")
    print(f"Total supply:    {fmt(total_supply, price)}")
    if total_sum == total_supply:
        print("MATCH: sum of balances equals total supply")
    else:
        print(f"MISMATCH: total_supply - sum = {total_supply - total_sum}")


if __name__ == "__main__":
    main()
