"""Focused, offline tests: never launch an app or contact the live upstream."""

import contextlib
import io
import json
import threading
import unittest
import urllib.error
import urllib.request
from unittest import mock

import bmux_infinite_scroll_acceptance as acceptance


def page(offset=0, total=300, limit=100, source="LIBRARY"):
    count = max(0, min(limit, total - offset))
    return (
        f"/menu/albums?offset={offset}&limit={limit}&sort=name",
        {
            "offset": offset,
            "limit": limit,
            "total": total,
            "items": [
                {
                    "albumId": offset + index,
                    "apiSource": source,
                    "title": f"Album {offset + index}",
                    "artist": "Artist",
                }
                for index in range(count)
            ],
        },
    )


class PageTests(unittest.TestCase):
    def test_three_pages_real_final(self):
        result = acceptance.validate_pages([page(0), page(100), page(200)])
        self.assertEqual(result["offsets"], [0, 100, 200])
        self.assertEqual(result["albums"], 300)
        self.assertTrue(result["final"])

    def test_bounded_run_not_final(self):
        self.assertFalse(acceptance.validate_pages([page(0, 500)])["final"])

    def test_partial_final(self):
        self.assertTrue(
            acceptance.validate_pages([page(0, 205), page(100, 205), page(200, 205)])[
                "final"
            ]
        )

    def test_unknown_total_not_fabricated_final(self):
        value = page()
        del value[1]["total"]
        self.assertFalse(acceptance.validate_pages([value])["final"])

    def test_duplicate_offset(self):
        with self.assertRaisesRegex(AssertionError, "Duplicate successful offsets"):
            acceptance.validate_pages([page(), page()])

    def test_skip_offset(self):
        with self.assertRaisesRegex(AssertionError, "Skipped page"):
            acceptance.validate_pages([page(), page(200)])

    def test_query_drift(self):
        path, body = page(100)
        with self.assertRaisesRegex(AssertionError, "query drift"):
            acceptance.validate_pages([page(), (path + "&search=changed", body)])

    def test_duplicate_album_identity(self):
        value = page(100)
        value[1]["items"][0]["albumId"] = 0
        with self.assertRaisesRegex(AssertionError, "Duplicate provider-scoped"):
            acceptance.validate_pages([page(), value])

    def test_provider_identity_is_scoped(self):
        self.assertNotEqual(
            acceptance.album_id({"albumId": 1, "apiSource": "A"}),
            acceptance.album_id({"albumId": 1, "apiSource": "B"}),
        )

    def test_zero_limit_fails(self):
        with self.assertRaisesRegex(AssertionError, "Nonprogressing|nonprogressing"):
            acceptance.validate_pages([page(limit=0)])

    def test_request_response_offset_mismatch(self):
        value = page()
        value[1]["offset"] = 100
        with self.assertRaisesRegex(AssertionError, "offset mismatch"):
            acceptance.validate_pages([value])

    def test_missing_pages_fails(self):
        with self.assertRaisesRegex(AssertionError, "No real"):
            acceptance.validate_pages([])

    def test_non_album_payload_ignored(self):
        self.assertEqual(acceptance.pages([("/health", {})]), [])

    def test_duplicate_visible_card_pair(self):
        value = page(total=1)
        value[1]["items"][0].update(title="Unique", artist="Artist")
        lines = ["" for _ in range(14)]
        title = acceptance.live.centered_ascii_label("Unique", 12)
        artist = acceptance.live.centered_ascii_label("Artist", 12)
        lines[8] = " " * 20 + title + " " * 8 + title
        lines[9] = " " * 20 + artist + " " * 8 + artist
        self.assertEqual(
            len(
                acceptance.duplicate_visible_titles(
                    "\n".join(lines), [value], [(20, 12), (40, 12)]
                )
            ),
            1,
        )

    def test_title_is_not_identity_without_artist(self):
        value = page(total=1)
        value[1]["items"][0].update(title="Unique", artist="Artist")
        lines = ["" for _ in range(14)]
        lines[8] = " " * 20 + acceptance.live.centered_ascii_label("Unique", 12)
        lines[9] = " " * 20 + acceptance.live.centered_ascii_label("Wrong", 12)
        self.assertEqual(
            acceptance.duplicate_visible_titles("\n".join(lines), [value], [(20, 12)]),
            [],
        )

    def test_checkpoint_required_before_side_effect(self):
        with mock.patch(
            "sys.argv",
            ["acceptance", "/not/a/binary", "--artifacts", "/not/an/artifact"],
        ), mock.patch.object(acceptance, "run") as run, contextlib.redirect_stderr(
            io.StringIO()
        ):
            with self.assertRaises(SystemExit):
                acceptance.main()
            run.assert_not_called()


class GatewayTests(unittest.TestCase):
    """Local HTTP only; replace upstream opener, not the allowlist."""

    def setUp(self):
        self.gateway = acceptance.Gateway("retry")
        self.gateway.opener = mock.Mock()
        response = mock.MagicMock()
        response.status = 200
        response.headers = {"Content-Type": "application/json"}
        response.read.return_value = json.dumps(page(100)[1]).encode()
        response.__enter__.return_value = response
        self.gateway.opener.open.return_value = response
        self.thread = threading.Thread(target=self.gateway.serve_forever, daemon=True)
        self.thread.start()
        self.client = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def tearDown(self):
        self.gateway.release.set()
        self.gateway.shutdown()
        self.gateway.server_close()
        self.thread.join(timeout=2)

    def request(self, path, method="GET", headers=None):
        request = urllib.request.Request(
            f"http://127.0.0.1:{self.gateway.server_port}{path}",
            method=method,
            headers=headers or {},
        )
        return self.client.open(request, timeout=2)

    def test_failure_once_then_real_forwarding(self):
        path = page(100)[0]
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.request(path)
        self.assertEqual(error.exception.code, 503)
        error.exception.close()
        self.gateway.opener.open.assert_not_called()
        with self.request(path) as response:
            self.assertEqual(response.status, 200)
        forwarded = self.gateway.opener.open.call_args.args[0]
        self.assertEqual(forwarded.full_url, acceptance.live.UPSTREAM + path)
        self.assertEqual(forwarded.method, "GET")
        self.assertEqual(len(self.gateway.payloads), 1)

    def test_mutation_and_unknown_route_denied_without_fault(self):
        for method, path in [
            ("POST", page(100)[0]),
            ("GET", "/config/save"),
            ("GET", "/menu/albums?url=http://evil"),
            ("GET", "/menu/albums?offset=1&offset=2"),
        ]:
            with self.subTest(method=method, path=path):
                with self.assertRaises(urllib.error.HTTPError) as error:
                    self.request(path, method)
                self.assertEqual(error.exception.code, 403)
                error.exception.close()
        self.gateway.opener.open.assert_not_called()
        self.assertFalse(self.gateway.fault_used)

    def test_upgrade_denied(self):
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.request(page(100)[0], headers={"Upgrade": "websocket"})
        self.assertEqual(error.exception.code, 403)
        error.exception.close()
        self.gateway.opener.open.assert_not_called()

    def test_album_request_budget_denies_before_forward(self):
        self.gateway.scenario = "pagination"
        self.gateway.max_album_requests = 1
        with self.request(page()[0]) as response:
            self.assertEqual(response.status, 200)
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.request(page(100)[0])
        self.assertEqual(error.exception.code, 403)
        error.exception.close()
        self.assertEqual(self.gateway.opener.open.call_count, 1)

    def test_total_budget_denies_before_forward(self):
        self.gateway.scenario = "pagination"
        self.gateway.max_total_requests = 1
        with self.request("/health") as response:
            self.assertEqual(response.status, 200)
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.request("/health")
        self.assertEqual(error.exception.code, 403)
        error.exception.close()
        self.assertEqual(self.gateway.opener.open.call_count, 1)
        self.assertEqual(self.gateway.budget_failures[-1]["budget"], "total_requests")
        self.assertEqual(len(self.gateway.requests), 2, "Never filter request history")

    def test_artwork_budget_preserves_album_capacity(self):
        self.gateway.scenario = "pagination"
        self.gateway.max_artwork_requests = 1
        with self.request("/files/albums/1/96x96") as response:
            self.assertEqual(response.status, 200)
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.request("/files/albums/2/96x96")
        self.assertEqual(error.exception.code, 403)
        error.exception.close()
        with self.request(page()[0]) as response:
            self.assertEqual(response.status, 200)
        self.assertEqual(self.gateway.opener.open.call_count, 2)
        self.assertEqual(self.gateway.budget_failures[-1]["budget"], "artwork_requests")

    def test_byte_budget_includes_inflight_reservations(self):
        self.gateway.max_bytes = acceptance.live.MAX_BODY
        self.gateway.reserved_bytes = acceptance.live.MAX_BODY
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.request("/health")
        self.assertEqual(error.exception.code, 403)
        error.exception.close()
        self.gateway.opener.open.assert_not_called()
        self.assertEqual(self.gateway.budget_failures[-1]["budget"], "bytes")

    def test_bytes_accounted_and_reservation_released(self):
        with self.request("/health") as response:
            body = response.read()
        self.assertEqual(self.gateway.forwarded_bytes, len(body))
        self.assertEqual(self.gateway.reserved_bytes, 0)

    def test_forward_strips_credentials_and_uses_fixed_profile(self):
        with self.request(
            "/health",
            headers={
                "Authorization": "local-test-not-secret",
                "Cookie": "local-test",
                "X-Extra": "no",
            },
        ):
            pass
        forwarded = self.gateway.opener.open.call_args.args[0]
        self.assertEqual(
            dict(forwarded.header_items()), {"Moosicbox-profile": "master"}
        )
        self.assertIsNone(forwarded.data)
        self.assertEqual(forwarded.full_url, acceptance.live.UPSTREAM + "/health")

    def test_delayed_response_waits_for_release(self):
        self.gateway.scenario = "stale"
        result = []

        def client():
            with self.request(page(100)[0]) as response:
                result.append(response.status)

        worker = threading.Thread(target=client)
        worker.start()
        self.assertTrue(self.gateway.fault_seen.wait(1))
        self.gateway.opener.open.assert_not_called()
        self.gateway.release.set()
        worker.join(timeout=2)
        self.assertFalse(worker.is_alive())
        self.assertEqual(result, [200])
        # HTTP headers/body can reach the client before the handler returns
        # from forwarding and publishes its completion event.
        self.assertTrue(self.gateway.delivered.wait(timeout=2))


if __name__ == "__main__":
    unittest.main()
