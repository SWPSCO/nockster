"""Exercise Play uploads and review-mode handling without contacting Google."""

import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlsplit


spec = importlib.util.spec_from_file_location(
    "android_upload_play",
    Path(__file__).resolve().parents[2] / "scripts/ci/android-upload-play.py",
)
play = importlib.util.module_from_spec(spec)
spec.loader.exec_module(play)

MANUAL_REVIEW = (
    "Changes cannot be sent for review automatically. Please set the query parameter "
    "changesNotSentForReview to true. Once committed, the changes in this edit can "
    "be sent for review from the Google Play Console UI."
)
AUTOMATIC_REVIEW = (
    "Changes are sent for review automatically. The query parameter "
    "changesNotSentForReview must not be set."
)


class AndroidUploadTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.bundle = Path(self.directory.name) / "app.aab"
        self.bundle.write_bytes(b"test bundle content")
        self.requests = []
        self.responses = [{"id": "edit-42"}, {"versionCode": 8601}, {}]
        self.client = play.PlayClient("test-token")
        self.addCleanup(patch.stopall)
        patch.object(play, "urlopen", side_effect=self.respond).start()
        patch("sys.stdout", new_callable=io.StringIO).start()

    def respond(self, request, timeout):
        self.assertEqual(timeout, 600)
        self.assertEqual(request.get_header("Authorization"), "Bearer test-token")
        self.assertEqual(urlsplit(request.full_url).hostname, "androidpublisher.googleapis.com")
        self.requests.append(request)
        response = self.responses.pop(0)
        if isinstance(response, tuple):
            code, message = response
            raise HTTPError(request.full_url, code, "API error", {},
                            io.BytesIO(json.dumps({"error": {"message": message}}).encode()))
        if isinstance(response, Exception):
            raise response
        return io.BytesIO(json.dumps(response).encode())

    def upload(self):
        return play.upload_release(self.client, self.bundle, "1.0.7", "8601")

    def assert_one_upload(self):
        uploads = [request for request in self.requests if "/bundles?" in request.full_url]
        self.assertEqual(len(uploads), 1)
        self.assertEqual(uploads[0].data, self.bundle.read_bytes())
        self.assertEqual(uploads[0].get_header("Content-type"), "application/octet-stream")
        self.assertTrue(uploads[0].full_url.endswith("/edits/edit-42/bundles?uploadType=media"))

    def test_automatic_review_omits_flag_and_uploads_completed_internal_release(self):
        self.responses.append({"id": "edit-42"})
        self.assertEqual(self.upload(), "automatic")
        self.assertEqual(len(self.requests), 4)
        self.assert_one_upload()
        track = self.requests[2]
        self.assertEqual(track.get_method(), "PUT")
        self.assertTrue(track.full_url.endswith("/edits/edit-42/tracks/internal"))
        self.assertEqual(json.loads(track.data), {
            "track": "internal",
            "releases": [{"name": "Nockster 1.0.7 (8601)", "versionCodes": ["8601"], "status": "completed"}],
        })
        commit = self.requests[-1]
        self.assertIsNone(commit.data)
        self.assertEqual(commit.get_method(), "POST")
        self.assertEqual(parse_qs(urlsplit(commit.full_url).query),
                         {"changesInReviewBehavior": ["ERROR_IF_IN_REVIEW"]})

    def test_manual_review_retries_only_commit_on_same_edit(self):
        self.responses.extend([(400, MANUAL_REVIEW), {"id": "edit-42"}])
        self.assertEqual(self.upload(), "manual")
        self.assertEqual(len(self.requests), 5)
        self.assert_one_upload()
        commits = self.requests[-2:]
        self.assertEqual(urlsplit(commits[0].full_url).path, urlsplit(commits[1].full_url).path)
        self.assertEqual(parse_qs(urlsplit(commits[1].full_url).query), {
            "changesInReviewBehavior": ["ERROR_IF_IN_REVIEW"],
            "changesNotSentForReview": ["true"],
        })
        self.assertIsNone(commits[1].data)

    def test_other_commit_errors_fail_without_retry(self):
        for status, message in [(403, "Permission denied"), (500, MANUAL_REVIEW),
                                (400, "You already have changes in review."),
                                (400, "App is still a draft"), (400, AUTOMATIC_REVIEW)]:
            with self.subTest(status=status, message=message):
                self.requests.clear()
                self.responses = [{"id": "edit-42"}, {"versionCode": 8601}, {}, (status, message)]
                with self.assertRaises(play.PlayAPIError):
                    self.upload()
                self.assertEqual(len(self.requests), 4)
                self.assert_one_upload()

    def test_retry_failure_is_bounded_and_preserves_api_error(self):
        self.responses.extend([(400, MANUAL_REVIEW), (400, AUTOMATIC_REVIEW)])
        with self.assertRaisesRegex(play.PlayAPIError, "must not be set"):
            self.upload()
        self.assertEqual(len(self.requests), 5)
        self.assert_one_upload()

    def test_wrong_version_does_not_update_track_or_commit(self):
        self.responses = [{"id": "edit-42"}, {"versionCode": 8501}]
        with self.assertRaisesRegex(ValueError, "does not match expected"):
            self.upload()
        self.assertEqual(len(self.requests), 2)

    def test_upload_failure_does_not_update_track_or_retry(self):
        for failure in [(400, "Version code already used"), URLError("connection lost")]:
            with self.subTest(failure=failure):
                self.requests.clear()
                self.responses = [{"id": "edit-42"}, failure]
                with self.assertRaises((play.PlayAPIError, URLError)):
                    self.upload()
                self.assertEqual(len(self.requests), 2)

    def test_invalid_inputs_fail_before_creating_edit(self):
        with self.assertRaisesRegex(ValueError, "ACCESS_TOKEN"):
            play.PlayClient("")
        for version, build in [("1.0", "8601"), ("1.0.7", "0"), ("1.0.7", "8601;id")]:
            with self.assertRaises(ValueError):
                play.upload_release(self.client, self.bundle, version, build)
        self.bundle.write_bytes(b"")
        with self.assertRaisesRegex(ValueError, "empty"):
            self.upload()
        self.assertEqual(self.requests, [])


if __name__ == "__main__":
    unittest.main()
