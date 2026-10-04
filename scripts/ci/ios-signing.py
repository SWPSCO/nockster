#!/usr/bin/env python3
"""Install per-run iOS signing material, upload an IPA, and clean the Mac runner."""
import base64
import datetime
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import secrets
import shlex
import shutil
import subprocess
import sys


def run(*args, **kwargs):
    return subprocess.check_output(args, **kwargs)


def required(name):
    value = os.environ.get(name)
    if not value:
        raise ValueError(f"Set the {name} Actions secret")
    return value


def decode_secret(name, destination):
    destination.write_bytes(base64.b64decode(''.join(required(name).split()), validate=True))
    destination.chmod(0o600)


def install(root):
    root.mkdir(parents=True, exist_ok=True)
    certificate = root / 'certificate.p12'
    profile_path = root / 'profile.mobileprovision'
    decode_secret('IOS_DISTRIBUTION_P12_B64', certificate)
    decode_secret('IOS_PROVISIONING_PROFILE_B64', profile_path)
    password = required('IOS_DISTRIBUTION_P12_PASSWORD')
    keychain = root / 'signing.keychain-db'
    search_list = shlex.split(run('security', 'list-keychains', '-d', 'user').decode())
    (root / 'keychains.json').write_text(json.dumps(search_list))
    keychain_password = secrets.token_urlsafe(32)
    run('security', 'create-keychain', '-p', keychain_password, str(keychain))
    run('security', 'set-keychain-settings', '-lut', '21600', str(keychain))
    run('security', 'unlock-keychain', '-p', keychain_password, str(keychain))
    # CMS decoding imports embedded certificates into the specified keychain.
    profile = plistlib.loads(run('security', 'cms', '-D', '-k', str(keychain), '-i', str(profile_path)))
    if profile['ExpirationDate'] <= datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None):
        raise ValueError('The iOS provisioning profile has expired')
    uuid = profile['UUID']
    if not re.fullmatch(r'[A-Fa-f0-9-]+', uuid):
        raise ValueError('Invalid provisioning profile UUID')
    team = profile['TeamIdentifier'][0]
    entitlements = profile['Entitlements']
    # The App ID prefix can differ from the Team ID for transferred accounts.
    app_id = entitlements.get('application-identifier', '')
    if app_id not in [f'{prefix}.io.swps.nockster' for prefix in profile['ApplicationIdentifierPrefix']]:
        raise ValueError('The profile must cover the explicit io.swps.nockster App ID')
    if entitlements.get('get-task-allow') or 'ProvisionedDevices' in profile or profile.get('ProvisionsAllDevices'):
        raise ValueError('Use an App Store distribution profile, not development, Ad Hoc, or enterprise')
    # Keep the signing chain available to the runner service in this keychain.
    # Public intermediate: https://www.apple.com/certificateauthority/AppleWWDRCAG3.cer
    intermediate = Path(__file__).resolve().parent / 'certificates/AppleWWDRCAG3.pem'
    run('security', 'import', str(intermediate), '-k', str(keychain))
    run('security', 'import', str(certificate), '-k', str(keychain), '-P', password,
        '-T', '/usr/bin/codesign', '-T', '/usr/bin/security')
    run('security', 'set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:',
        '-k', keychain_password, str(keychain))
    run('security', 'list-keychains', '-d', 'user', '-s', str(keychain), *search_list)
    identities = run('security', 'find-identity', '-v', '-p', 'codesigning', str(keychain)).decode()
    allowed = {hashlib.sha1(cert).hexdigest().upper() for cert in profile['DeveloperCertificates']}
    matches = [sha for sha, _ in re.findall(r'([A-F0-9]{40}) "([^"]+)"', identities) if sha in allowed]
    if len(matches) != 1:
        raise ValueError('The P12 must contain one valid signing identity included in the App Store profile')
    # Xcode 26 reads profiles from this directory. A run-specific filename leaves
    # profiles belonging to the developer or other projects untouched.
    profile_dir = Path.home() / 'Library/Developer/Xcode/UserData/Provisioning Profiles'
    profile_dir.mkdir(parents=True, exist_ok=True)
    installed = profile_dir / f'nockster-{os.environ["GITHUB_RUN_ID"]}-{os.environ["GITHUB_RUN_ATTEMPT"]}.mobileprovision'
    if installed.exists():
        raise ValueError('The per-run provisioning profile already exists')
    (root / 'installed-profile.json').write_text(json.dumps(str(installed)))
    shutil.copyfile(profile_path, installed)
    installed.chmod(0o600)
    options = {'method': 'app-store-connect', 'destination': 'export', 'signingStyle': 'manual',
               'teamID': team, 'signingCertificate': matches[0],
               'provisioningProfiles': {'io.swps.nockster': uuid},
               'manageAppVersionAndBuildNumber': False, 'uploadSymbols': True}
    (root / 'ExportOptions.plist').write_bytes(plistlib.dumps(options))
    with open(os.environ['GITHUB_ENV'], 'a') as output:
        output.write(f'IOS_TEAM_ID={team}\nIOS_PROFILE_UUID={uuid}\nIOS_SIGNING_SHA1={matches[0]}\n')
    certificate.unlink()
    profile_path.unlink()


def upload(root):
    key_id = required('AC_API_KEY_ID')
    issuer = required('AC_API_ISSUER_ID')
    if not re.fullmatch(r'[A-Za-z0-9]+', key_id):
        raise ValueError('Invalid App Store Connect key ID')
    upload_dir = root / 'upload'
    key_dir = upload_dir / 'private_keys'
    key_dir.mkdir(parents=True, exist_ok=True)
    key = key_dir / f'AuthKey_{key_id}.p8'
    decode_secret('AC_API_KEY_PATH', key)
    ipa = Path(f'release/ios/nockster-ios-{os.environ["NOCKSTER_VERSION"]}-{os.environ["NOCKSTER_BUILD_NUMBER"]}.ipa').resolve()
    try:
        subprocess.run(['xcrun', 'altool', '--upload-app', '--type', 'ios', '--file', str(ipa),
                        '--apiKey', key_id, '--apiIssuer', issuer], cwd=upload_dir, check=True)
    finally:
        shutil.rmtree(upload_dir)


def cleanup(root):
    search_list = root / 'keychains.json'
    if search_list.exists():
        run('security', 'list-keychains', '-d', 'user', '-s', *json.loads(search_list.read_text()))
    keychain = root / 'signing.keychain-db'
    if keychain.exists():
        run('security', 'delete-keychain', str(keychain))
    installed = root / 'installed-profile.json'
    if installed.exists():
        Path(json.loads(installed.read_text())).unlink(missing_ok=True)
    shutil.rmtree(root, ignore_errors=True)


if __name__ == '__main__':
    os.umask(0o077)
    try:
        {'install': install, 'upload': upload, 'cleanup': cleanup}[sys.argv[1]](Path(os.environ['IOS_BUILD_ROOT']))
    except subprocess.CalledProcessError as error:
        # Arguments to security import include passwords; do not print the command.
        print(f'::error::{error.cmd[0]} failed with exit status {error.returncode}', file=sys.stderr)
        sys.exit(1)
    except (ValueError, KeyError) as error:
        print(f'::error::{error}', file=sys.stderr)
        sys.exit(1)
