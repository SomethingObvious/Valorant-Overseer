# Copyright 2026 SomethingObvious. PolyForm Strict 1.0.0, see LICENSE.md.
"""Which Overseer release is newest, for the window's look once a day.

One HEAD request to GitHub, which answers with a redirect to the newest
release's page. The redirect is read and never followed, and a HEAD answer
has no body, so nothing is downloaded and nothing goes out but the request.
No session is kept, so the cookies GitHub sets are dropped with it. The only
thing taken from the answer is a version number in exactly the expected
place, and anything else counts as no answer.
"""

from __future__ import annotations

import re
from typing import Any

import overseerlog
import requests
from common.check import check

LOG = overseerlog.get_logger("backend")

_RELEASES = "https://github.com/SomethingObvious/Valorant-Overseer/releases/"
_LATEST = _RELEASES + "latest"
_TAG = re.compile(re.escape(_RELEASES + "tag/v") + r"(\d{1,4})\.(\d{1,4})\.(\d{1,4})")
_REDIRECTS = (301, 302, 303, 307, 308)


def version(location: str) -> str | None:
    """Return the version a redirect names, or None when it is anything else."""
    found = _TAG.fullmatch(location)
    return ".".join(str(int(part)) for part in found.groups()) if found else None


def newest() -> str | None:
    """Return the newest release's version, or None when GitHub can't say."""
    try:
        answer = requests.head(
            _LATEST,
            allow_redirects=False,
            timeout=5,
            headers={"User-Agent": "Valorant-Overseer"},
        )
    except requests.RequestException as e:
        LOG.info("could not ask which release is newest: %r", e)
        return None
    if answer.status_code not in _REDIRECTS:
        return None
    return version(answer.headers.get("Location", ""))


def _self_check() -> None:
    from unittest import mock

    page = _RELEASES + "tag/"
    check(version(page + "v2.10.0") == "2.10.0")
    check(version(page + "v02.1.0") == "2.1.0")
    for bad in (
        "",
        page + "v2.1",
        page + "v2.1.0-beta",
        page + "v2.1.0\n",
        page + "v2.1.0/../../download",
        page + "v12345.1.0",
        page.replace("https:", "http:") + "v9.9.9",
        page.replace("github.com", "github.com.evil.example") + "v9.9.9",
        page.replace("SomethingObvious", "someone-else") + "v9.9.9",
    ):
        check(version(bad) is None, bad)

    asked: list[dict[str, Any]] = []

    def answering(status: int, location: str) -> Any:
        def head(url: str, **options: Any) -> Any:
            asked.append({"url": url, **options})
            return mock.Mock(status_code=status, headers={"Location": location})

        return head

    with mock.patch("requests.head", side_effect=answering(302, page + "v2.2.0")):
        check(newest() == "2.2.0")
    check(asked[-1]["url"] == _LATEST and asked[-1]["allow_redirects"] is False)
    with mock.patch("requests.head", side_effect=answering(200, page + "v2.2.0")):
        check(newest() is None, "only a redirect is an answer")
    with mock.patch("requests.head", side_effect=requests.ConnectionError("offline")):
        check(newest() is None)
    print("newest self-check OK (versions, strange answers, offline)")


if __name__ == "__main__":
    _self_check()
