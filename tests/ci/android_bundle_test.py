"""Release bundle checks reject unoptimized code and mismatched crash maps."""

import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest
import zipfile


verify_bundle = runpy.run_path(
    Path(__file__).resolve().parents[2] / "scripts/ci/android-verify-bundle.py"
)["verify_bundle"]


class AndroidBundleTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.bundle = Path(self.directory.name) / "app.aab"
        self.mapping = Path(self.directory.name) / "mapping.txt"
        self.mapping.write_text("example.Wallet -> a.b:\n")
        self.metadata = {
            "options": {
                "isObfuscationEnabled": True,
                "isOptimizationsEnabled": True,
                "isShrinkingEnabled": True,
            },
            "stats": {
                "noObfuscationPercentage": 5.46,
                "noOptimizationPercentage": 6.53,
                "noShrinkingPercentage": 5.37,
            },
        }

    def write_bundle(self, metadata, dex=b"dex", mapping=None):
        with zipfile.ZipFile(self.bundle, "w") as bundle:
            bundle.writestr("base/dex/classes.dex", dex)
            bundle.writestr(
                "BUNDLE-METADATA/com.android.tools.build.obfuscation/proguard.map",
                self.mapping.read_bytes() if mapping is None else mapping,
            )
            if metadata is not None:
                bundle.writestr("BUNDLE-METADATA/com.android.tools/r8.json", json.dumps(metadata))

    def test_reports_optimized_bundle_and_matches_mapping(self):
        self.write_bundle(self.metadata)
        self.assertEqual(verify_bundle(self.bundle, self.mapping), (
            3, {"Obfuscation": 94.54, "Optimization": 93.47, "Shrinking": 94.63}
        ))

    def test_enforces_each_category_at_threshold(self):
        for field in self.metadata["stats"]:
            for excluded, passes in [(75, True), (75.001, False), (75.01, False), (100, False)]:
                with self.subTest(field=field, excluded=excluded):
                    metadata = copy.deepcopy(self.metadata)
                    metadata["stats"][field] = excluded
                    self.write_bundle(metadata)
                    if passes:
                        verify_bundle(self.bundle, self.mapping)
                    else:
                        with self.assertRaisesRegex(ValueError, "at least 25%"):
                            verify_bundle(self.bundle, self.mapping)

    def test_rejects_disabled_optimization_and_invalid_stats(self):
        for option in self.metadata["options"]:
            metadata = copy.deepcopy(self.metadata)
            metadata["options"][option] = False
            self.write_bundle(metadata)
            with self.assertRaisesRegex(ValueError, "disabled"):
                verify_bundle(self.bundle, self.mapping)
        for excluded in [float("nan"), -1, 101, "5", True]:
            metadata = copy.deepcopy(self.metadata)
            metadata["stats"]["noObfuscationPercentage"] = excluded
            self.write_bundle(metadata)
            with self.assertRaisesRegex(ValueError, "Invalid R8"):
                verify_bundle(self.bundle, self.mapping)

    def test_rejects_missing_metadata_dex_and_wrong_mapping(self):
        self.write_bundle(None)
        with self.assertRaises(KeyError):
            verify_bundle(self.bundle, self.mapping)
        self.write_bundle(self.metadata, dex=b"")
        with self.assertRaisesRegex(ValueError, "no DEX"):
            verify_bundle(self.bundle, self.mapping)
        self.write_bundle(self.metadata, mapping=b"different")
        with self.assertRaisesRegex(ValueError, "do not match"):
            verify_bundle(self.bundle, self.mapping)
        self.mapping.write_text("")
        with self.assertRaisesRegex(ValueError, "empty"):
            verify_bundle(self.bundle, self.mapping)


if __name__ == "__main__":
    unittest.main()
