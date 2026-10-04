#!/usr/bin/env python3
"""Verify the optimization metadata and crash mapping of a release bundle."""

import json
import math
from pathlib import Path
import sys
import zipfile


def verify_bundle(bundle_path, mapping_path):
    mapping = Path(mapping_path).read_bytes()
    if not mapping.strip():
        raise ValueError("R8 mapping file is empty")
    with zipfile.ZipFile(bundle_path) as bundle:
        if bundle.read("BUNDLE-METADATA/com.android.tools.build.obfuscation/proguard.map") != mapping:
            raise ValueError("Bundle and external R8 mapping files do not match")
        metadata = json.loads(bundle.read("BUNDLE-METADATA/com.android.tools/r8.json"))
        dex_size = sum(entry.file_size for entry in bundle.infolist() if entry.filename.endswith(".dex"))
    if not dex_size:
        raise ValueError("Bundle contains no DEX code")
    percentages = {}
    for category in ("Obfuscation", "Optimization", "Shrinking"):
        option = "isOptimizationsEnabled" if category == "Optimization" else f"is{category}Enabled"
        if metadata["options"].get(option) is not True:
            raise ValueError(f"R8 {category.lower()} is disabled")
        excluded = metadata["stats"][f"no{category}Percentage"]
        if type(excluded) not in (int, float) or not math.isfinite(excluded) or not 0 <= excluded <= 100:
            raise ValueError(f"Invalid R8 {category.lower()} percentage")
        percentage = 100 - excluded
        if percentage < 25:
            raise ValueError(f"R8 {category.lower()} is {percentage:.2f}%; at least 25% is required")
        percentages[category] = round(percentage, 2)
    return dex_size, percentages


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("Usage: android-verify-bundle.py BUNDLE.aab MAPPING.txt")
    try:
        dex_size, percentages = verify_bundle(*sys.argv[1:])
    except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile) as error:
        sys.exit(f"Android release verification failed: {error}")
    print(f"Android release DEX size: {dex_size / 1_000_000:.2f} MB")
    for category, percentage in percentages.items():
        print(f"R8 {category.lower()}: {percentage:.2f}%")
    print("Bundle and external crash mapping files match.")
