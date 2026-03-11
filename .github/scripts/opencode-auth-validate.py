#!/usr/bin/env python3

import datetime
import json
import pathlib
import sys
import urllib.error
import urllib.request


def main() -> int:
    if len(sys.argv) != 2:
        print("valid=false")
        print("needs_refresh=true")
        print("reason=invalid_args")
        return 0

    path = pathlib.Path(sys.argv[1])
    data = json.loads(path.read_text())
    openai = data.get("openai")
    if openai is None and isinstance(data.get("providers"), dict):
        openai = data["providers"].get("openai")

    if not isinstance(openai, dict):
        print("valid=false")
        print("needs_refresh=true")
        print("reason=missing_openai_provider")
        return 0

    access = openai.get("access")
    refresh = openai.get("refresh")
    expires = openai.get("expires")

    if not isinstance(access, str) or not access.strip():
        print("valid=false")
        print("needs_refresh=true")
        print("reason=missing_access_token")
        return 0

    if not isinstance(refresh, str) or not refresh.strip():
        print("valid=false")
        print("needs_refresh=true")
        print("reason=missing_refresh_token")
        return 0

    expired = False
    if isinstance(expires, (int, float)):
        sec = expires / 1000 if expires > 10_000_000_000 else expires
        expiry = datetime.datetime.fromtimestamp(sec, datetime.UTC)
        expired = expiry <= (
            datetime.datetime.now(datetime.UTC) + datetime.timedelta(minutes=5)
        )

    if expired:
        print("valid=false")
        print("needs_refresh=true")
        print("reason=expired_or_expiring")
        return 0

    req = urllib.request.Request(
        "https://api.openai.com/v1/models",
        headers={"Authorization": f"Bearer {access.strip()}"},
    )

    status = None
    for attempt in range(2):
        try:
            with urllib.request.urlopen(req, timeout=20) as res:
                status = int(res.status)
                break
        except urllib.error.HTTPError as err:
            status = int(err.code)
            if status in (429, 500, 502, 503, 504) and attempt == 0:
                continue
            break
        except Exception:
            if attempt == 0:
                continue
            status = -1

    if status in (200, 403):
        print("valid=true")
        print("needs_refresh=false")
        print(f"reason=http_{status}")
    elif status == 401:
        print("valid=false")
        print("needs_refresh=true")
        print("reason=http_401")
    else:
        print("valid=false")
        print("needs_refresh=true")
        print(f"reason=http_{status}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
