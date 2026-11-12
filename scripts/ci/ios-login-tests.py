#!/usr/bin/env python3
"""Run the bundled wallet's login regression on disposable review-device simulators.

Requires `npm run mobile:sync` and Xcode 26+ with an iOS simulator runtime.
Usage: python3 scripts/ci/ios-login-tests.py /tmp/nockster-ios-login
"""

import json
import os
from pathlib import Path
import subprocess
import sys


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def main():
    os.environ.setdefault("DEVELOPER_DIR", "/Applications/Xcode.app/Contents/Developer")
    build_root = Path(sys.argv[1]).resolve()
    build_root.mkdir(parents=True, exist_ok=True)
    runtimes = json.loads(output("xcrun", "simctl", "list", "runtimes", "--json"))["runtimes"]
    runtime = max(
        (r for r in runtimes if r["isAvailable"] and r["identifier"].startswith("com.apple.CoreSimulator.SimRuntime.iOS-")
         and int(r["version"].split(".")[0]) >= 26),
        key=lambda r: tuple(map(int, r["version"].split("."))),
    )
    devices = []
    try:
        for model in ("iPhone-17-Pro-Max", "iPad-Air-11-inch-M3"):
            devices.append(output(
                "xcrun", "simctl", "create", f"Nockster login test {model}",
                f"com.apple.CoreSimulator.SimDeviceType.{model}", runtime["identifier"],
            ))
        command = [
            "xcodebuild", "test", "-project", "apps/mobile/ios/App/App.xcodeproj",
            "-scheme", "AppTests", "-parallel-testing-enabled", "NO",
            "-disable-concurrent-destination-testing",
            "-test-timeouts-enabled", "YES", "-maximum-test-execution-time-allowance", "120",
            "-derivedDataPath", str(build_root / "derived"),
            "-resultBundlePath", str(build_root / "login.xcresult"),
            "CODE_SIGNING_ALLOWED=NO",
        ]
        for device in devices:
            command.extend(["-destination", f"platform=iOS Simulator,id={device}"])
        subprocess.run(command, check=True)
    finally:
        for device in devices:
            subprocess.run(["xcrun", "simctl", "shutdown", device], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            subprocess.run(["xcrun", "simctl", "delete", device], check=True)


if __name__ == "__main__":
    main()
