#!/usr/bin/env python3
"""Upload one Nockster bundle and commit its Internal testing release."""

import json
import os
from pathlib import Path
import re
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


PACKAGE = "io.swps.nockster"
API_ROOT = "https://androidpublisher.googleapis.com"


class PlayAPIError(Exception):
    def __init__(self, status, message):
        super().__init__(f"Google Play HTTP {status}: {message}")
        self.status = status
        self.message = message


class PlayClient:
    def __init__(self, token):
        if not token:
            raise ValueError("GOOGLE_PLAY_ACCESS_TOKEN is required")
        self.token = token

    def request(self, method, path, payload=None, media=None):
        headers = {"Authorization": f"Bearer {self.token}"}
        data = None
        if media is not None:
            data = media
            headers["Content-Type"] = "application/octet-stream"
        elif payload is not None:
            data = json.dumps(payload).encode()
            headers["Content-Type"] = "application/json"
        request = Request(API_ROOT + path, data=data, headers=headers, method=method)
        try:
            with urlopen(request, timeout=600) as response:
                body = response.read()
                return json.loads(body) if body else {}
        except HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            try:
                message = json.loads(body)["error"]["message"]
            except (ValueError, KeyError, TypeError):
                message = body or error.reason
            raise PlayAPIError(error.code, str(message)) from None


def commit_edit(client, edit_path):
    # Preserve reviews already in progress. Only the explicit review-mode error
    # permits a second commit request; the edit and uploaded bundle stay the same.
    path = f"{edit_path}:commit?changesInReviewBehavior=ERROR_IF_IN_REVIEW"
    try:
        client.request("POST", path)
        return "automatic"
    except PlayAPIError as error:
        manual_review_required = error.status == 400 and re.search(
            r"Changes cannot be sent for review automatically\..*"
            r"changesNotSentForReview.*\btrue\b",
            error.message,
            re.IGNORECASE | re.DOTALL,
        )
        if not manual_review_required:
            raise
    print("Play requires manual review submission; retrying the same edit with changesNotSentForReview=true.")
    client.request("POST", path + "&changesNotSentForReview=true")
    return "manual"


def upload_release(client, bundle_path, version, build_number):
    if not re.fullmatch(r"[1-9]\d*", build_number):
        raise ValueError("Build number must be a positive integer")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Version must contain three numeric components")
    bundle = Path(bundle_path)
    if bundle.suffix != ".aab":
        raise ValueError("Release artifact must be an Android App Bundle (.aab)")
    media = bundle.read_bytes()
    if not media:
        raise ValueError("Release bundle is empty")
    edits_path = f"/androidpublisher/v3/applications/{PACKAGE}/edits"
    edit = client.request("POST", edits_path, payload={})
    edit_path = f"{edits_path}/{quote(str(edit['id']), safe='')}"
    print(f"Created Google Play edit {edit['id']}; uploading {bundle.name}.")
    uploaded = client.request("POST", f"/upload{edit_path}/bundles?uploadType=media", media=media)
    if str(uploaded["versionCode"]) != build_number:
        raise ValueError(f"Uploaded version code {uploaded['versionCode']} does not match expected {build_number}")
    client.request("PUT", f"{edit_path}/tracks/internal", payload={
        "track": "internal",
        "releases": [{
            "name": f"Nockster {version} ({build_number})",
            "versionCodes": [build_number],
            "status": "completed",
        }],
    })
    return commit_edit(client, edit_path)


def main():
    if len(sys.argv) != 4:
        sys.exit("Usage: android-upload-play.py BUNDLE.aab VERSION BUILD_NUMBER")
    try:
        mode = upload_release(PlayClient(os.environ.get("GOOGLE_PLAY_ACCESS_TOKEN")), *sys.argv[1:])
    except (PlayAPIError, OSError, URLError, ValueError, KeyError) as error:
        sys.exit(f"Google Play release failed: {error}")
    summary = (
        f"Google Play accepted Nockster {sys.argv[2]} (version code {sys.argv[3]}) "
        f"for **Internal testing**.\n\nPackage: `{PACKAGE}`.\n\n"
    )
    if mode == "manual":
        summary += "Open **Publishing overview** in Play Console and send the changes for review.\n"
    else:
        summary += "Play accepted the edit with automatic review handling.\n"
    summary += "\nTester availability depends on Play processing, app setup, and any required approval.\n"
    print(summary)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as output:
            output.write(summary)


if __name__ == "__main__":
    main()
