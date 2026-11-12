#!/usr/bin/env python3
"""Capture the native app using a seed file and disposable iPhone/iPad simulators.

Requires npm run mobile:sync. Recovery material is read at runtime, never passed
on the command line. Disposable simulators are deleted on success or failure.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


SCREENSHOTS = {'01-wallet', '02-receive', '03-activity', '04-send', '05-wallet-dark'}
DEVICES = [('iphone', 'iPhone-17-Pro-Max'), ('ipad', 'iPad-Pro-13-inch-M5-12GB')]


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def capture(device, seed, directory, build):
    directory.mkdir(parents=True, exist_ok=True)
    request = directory / 'capture-request.txt'
    request.unlink(missing_ok=True)
    env = {
        **os.environ,
        'TEST_RUNNER_NOCKSTER_SCREENSHOT_SEED': str(seed),
        'TEST_RUNNER_NOCKSTER_SCREENSHOT_OUTPUT': str(directory),
    }
    command = [
        'xcodebuild', 'test', '-project', 'apps/mobile/ios/App/App.xcodeproj',
        '-scheme', 'AppTests', '-destination', f'platform=iOS Simulator,id={device}',
        '-derivedDataPath', str(build), '-parallel-testing-enabled', 'NO',
        '-only-testing:AppTests/WalletScreenshotTests',
        '-test-timeouts-enabled', 'YES', '-maximum-test-execution-time-allowance', '180',
        '-testLanguage', 'en', '-testRegion', 'US', 'CODE_SIGNING_ALLOWED=NO',
    ]
    process = subprocess.Popen(command, env=env)
    captured = set()
    try:
        deadline = time.monotonic() + 600
        while process.poll() is None:
            if time.monotonic() > deadline:
                raise TimeoutError('Screenshot test exceeded ten minutes')
            if request.exists():
                shot = request.read_text().strip()
                if shot not in SCREENSHOTS:
                    raise ValueError('Unexpected screenshot name')
                subprocess.run([
                    'xcrun', 'simctl', 'io', device, 'screenshot', '--type=png',
                    str(directory / (shot + '.png')),
                ], check=True, timeout=25)
                captured.add(shot)
                request.unlink()
            time.sleep(0.2)
        if process.returncode:
            raise subprocess.CalledProcessError(process.returncode, process.args)
        if captured != SCREENSHOTS:
            raise RuntimeError('Capture did not produce all five screenshots')
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('seed', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--device', choices=['all', 'iphone', 'ipad'], default='all')
    args = parser.parse_args()
    seed, destination = args.seed.resolve(), args.destination.resolve()
    if not seed.is_file():
        raise SystemExit('Seed file does not exist')
    destination.mkdir(parents=True, exist_ok=True)
    os.environ.setdefault('DEVELOPER_DIR', '/Applications/Xcode.app/Contents/Developer')
    runtimes = json.loads(output('xcrun', 'simctl', 'list', 'runtimes', '--json'))['runtimes']
    available = [
        r for r in runtimes if r['isAvailable']
        and r['identifier'].startswith('com.apple.CoreSimulator.SimRuntime.iOS-')
        and int(r['version'].split('.')[0]) >= 26
    ]
    if not available:
        raise SystemExit('Install an iOS 26 or newer simulator runtime first')
    runtime = max(available, key=lambda r: tuple(map(int, r['version'].split('.'))))
    with tempfile.TemporaryDirectory(prefix='nockster-store-build-') as build:
        for name, kind in DEVICES:
            if args.device not in ('all', name):
                continue
            device = output(
                'xcrun', 'simctl', 'create', f'Nockster store {name}',
                f'com.apple.CoreSimulator.SimDeviceType.{kind}', runtime['identifier'],
            )
            try:
                subprocess.run(['xcrun', 'simctl', 'boot', device], check=True)
                subprocess.run(['xcrun', 'simctl', 'bootstatus', device, '-b'], check=True, timeout=180)
                subprocess.run([
                    'xcrun', 'simctl', 'status_bar', device, 'override', '--time', '9:41',
                    '--dataNetwork', 'wifi', '--wifiMode', 'active', '--wifiBars', '3',
                    '--batteryState', 'charged', '--batteryLevel', '100',
                ], check=True)
                # Simulator services cannot write directly into macOS-protected
                # Documents folders. Capture in temporary storage, then export.
                directory = Path(build) / 'captures' / name
                capture(device, seed, directory, build)
                (destination / name).mkdir(exist_ok=True)
                for shot in sorted(SCREENSHOTS):
                    subprocess.run([
                        'node', 'scripts/ci/export-store-png.mjs',
                        str(directory / (shot + '.png')),
                        str(destination / name / (shot + '.png')),
                    ], check=True)
            finally:
                subprocess.run(['xcrun', 'simctl', 'shutdown', device],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                subprocess.run(['xcrun', 'simctl', 'delete', device], check=True)


if __name__ == '__main__':
    main()
