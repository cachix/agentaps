import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch
from urllib.error import HTTPError


spec = importlib.util.spec_from_file_location(
    "publish_crates", Path(__file__).parents[1] / "publish-crates.py"
)
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)


class PublishTests(unittest.TestCase):
    def run_release(self, published, tag="v0.5.3", dry_run=False, terminal=True):
        metadata = {
            "packages": [
                {"name": "agentaps", "version": "0.5.3", "publish": None},
                {"name": "gpui-rio", "version": "0.1.0", "publish": None},
                {
                    "name": "agentaps-control-protocol",
                    "version": "0.1.0",
                    "publish": None,
                },
            ]
        }
        if not terminal:
            metadata["packages"] = [p for p in metadata["packages"] if p["name"] != "gpui-rio"]
        args = ["--tag", tag]
        if dry_run:
            args.append("--dry-run")
        else:
            args.append("--publish")
        with (
            patch.object(publisher.subprocess, "run") as run,
            patch.object(publisher, "registry_has_version", side_effect=published),
        ):
            run.return_value.stdout = json.dumps(metadata)
            publisher.main(args)
            return [call.args[0] for call in run.call_args_list[1:]]

    def test_existing_protocol_is_not_republished_with_a_new_desktop_release(self):
        commands = self.run_release([True, True, False])
        self.assertEqual(
            commands,
            [["cargo", "publish", "--locked", "--registry", "crates-io", "--package", "agentaps"]],
        )

    def test_retry_after_all_uploads_does_not_upload_again(self):
        self.assertEqual(self.run_release([True, True, True]), [])

    def test_first_publication_selects_dependencies_and_desktop_together(self):
        commands = self.run_release([False, False, False])
        self.assertEqual(
            commands,
            [[
                "cargo", "publish", "--locked", "--registry", "crates-io",
                "--package", "agentaps-control-protocol", "--package", "gpui-rio",
                "--package", "agentaps",
            ]],
        )

    def test_dry_run_still_validates_packages_for_an_existing_release(self):
        self.assertEqual(
            self.run_release([True, True, True], dry_run=True),
            [["cargo", "publish", "--workspace", "--dry-run", "--locked", "--registry", "crates-io"]],
        )

    def test_release_before_rio_extraction_can_still_be_retried(self):
        commands = self.run_release([True, False], terminal=False)
        self.assertEqual(commands, [[
            "cargo", "publish", "--locked", "--registry", "crates-io",
            "--package", "agentaps",
        ]])

    def test_new_terminal_version_is_published_before_desktop(self):
        commands = self.run_release([True, False, False])
        self.assertEqual(commands, [[
            "cargo", "publish", "--locked", "--registry", "crates-io",
            "--package", "gpui-rio", "--package", "agentaps",
        ]])

    def test_mismatched_version_cannot_publish(self):
        with self.assertRaisesRegex(ValueError, "match"):
            self.run_release([], tag="v0.5.4")

    def test_prerelease_tag_cannot_publish(self):
        with self.assertRaisesRegex(ValueError, "stable"):
            self.run_release([], tag="v0.5.3-rc.1")

    def test_registry_failure_cannot_be_mistaken_for_an_unpublished_version(self):
        error = HTTPError("https://index.crates.io", 503, "unavailable", {}, None)
        with patch.object(publisher, "urlopen", side_effect=error):
            with self.assertRaises(HTTPError):
                publisher.registry_has_version("agentaps", "0.5.3")

    def test_missing_index_allows_a_crates_first_publication(self):
        error = HTTPError("https://index.crates.io", 404, "not found", {}, None)
        with patch.object(publisher, "urlopen", side_effect=error):
            self.assertFalse(publisher.registry_has_version("agentaps", "0.5.3"))

    def test_yanked_versions_cannot_be_skipped_as_successful_publications(self):
        response = io.BytesIO(b'{"vers":"0.5.3","yanked":true}\n')
        with patch.object(publisher, "urlopen", return_value=response):
            with self.assertRaisesRegex(ValueError, "yanked"):
                publisher.registry_has_version("agentaps", "0.5.3")


if __name__ == "__main__":
    unittest.main()
