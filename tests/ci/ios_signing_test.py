import base64
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('ios_signing', Path(__file__).resolve().parents[2] / 'scripts/ci/ios-signing.py')
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class SigningTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / 'build'
        self.home = Path(self.temporary.name) / 'home'
        self.certificate = b'fixture certificate'
        self.sha = hashlib.sha1(self.certificate).hexdigest().upper()
        self.identity_name = 'Apple Distribution: Nockster (TEAM123)'
        self.profile = {
            'ExpirationDate': datetime.datetime(2099, 1, 1),
            'UUID': '12345678-ABCD-1234-ABCD-123456789012',
            'TeamIdentifier': ['TEAM123'],
            'ApplicationIdentifierPrefix': ['PREFIX123'],
            'Entitlements': {'application-identifier': 'PREFIX123.io.swps.nockster', 'get-task-allow': False},
            'DeveloperCertificates': [self.certificate],
        }
        self.calls = []
        self.env = {
            'IOS_DISTRIBUTION_P12_B64': base64.b64encode(b'fixture p12').decode(),
            'IOS_PROVISIONING_PROFILE_B64': base64.b64encode(b'fixture profile').decode(),
            'IOS_DISTRIBUTION_P12_PASSWORD': 'fixture password',
            'GITHUB_ENV': str(Path(self.temporary.name) / 'env'),
            'GITHUB_RUN_ID': '123', 'GITHUB_RUN_ATTEMPT': '1',
        }
        self.patchers = [patch.dict(os.environ, self.env), patch.object(Path, 'home', return_value=self.home),
                         patch.object(signing, 'run', side_effect=self.command)]
        for patcher in self.patchers:
            patcher.start()
            self.addCleanup(patcher.stop)

    def command(self, *args):
        self.calls.append(args)
        if args[1] == 'cms':
            if '-k' not in args or not Path(args[args.index('-k') + 1]).exists():
                raise subprocess.CalledProcessError(1, args, stderr=b'A default keychain could not be found')
            return plistlib.dumps(self.profile)
        if args[1] == 'list-keychains' and '-s' not in args:
            return b'"/Users/builder/Library/Keychains/login.keychain-db"\n'
        if args[1] == 'find-identity':
            return f'1) {self.sha} "{self.identity_name}"'.encode()
        if args[1] == 'create-keychain':
            Path(args[-1]).touch()
        return b''

    def test_install_and_cleanup_preserve_the_runner_state(self):
        signing.install(self.root)
        installed = Path(json.loads((self.root / 'installed-profile.json').read_text()))
        self.assertTrue(installed.exists())
        self.assertEqual(installed.stat().st_mode & 0o777, 0o600)
        self.assertFalse((self.root / 'certificate.p12').exists())
        options = plistlib.loads((self.root / 'ExportOptions.plist').read_bytes())
        self.assertEqual(options['signingCertificate'], self.sha)
        self.assertEqual(options['teamID'], 'TEAM123')
        self.assertFalse(options['manageAppVersionAndBuildNumber'])
        self.assertIn('IOS_PROFILE_UUID=', Path(self.env['GITHUB_ENV']).read_text())
        unrelated = installed.parent / 'developer.mobileprovision'
        unrelated.write_bytes(b'developer profile')
        signing.cleanup(self.root)
        self.assertFalse(installed.exists())
        self.assertEqual(unrelated.read_bytes(), b'developer profile')
        self.assertFalse(self.root.exists())
        self.assertIn(('security', 'list-keychains', '-d', 'user', '-s', '/Users/builder/Library/Keychains/login.keychain-db'), self.calls)
        self.assertTrue(any(call[1] == 'delete-keychain' for call in self.calls))

    def test_install_uses_a_private_keychain_without_setting_a_default(self):
        signing.install(self.root)
        self.assertIn(('security', 'cms', '-D', '-k', str(self.root / 'signing.keychain-db'),
                       '-i', str(self.root / 'profile.mobileprovision')), self.calls)
        self.assertFalse(any(call[1] == 'default-keychain' for call in self.calls))
        signing.cleanup(self.root)
        self.assertFalse(self.root.exists())

    def test_identity_selection_uses_the_profile_certificate(self):
        self.identity_name = 'iPhone Distribution: Southwestern Pool Supply CO INC (TEAM123)'
        signing.install(self.root)
        options = plistlib.loads((self.root / 'ExportOptions.plist').read_bytes())
        self.assertEqual(options['signingCertificate'], self.sha)
        signing.cleanup(self.root)

    def test_signing_keychain_contains_the_distribution_chain(self):
        signing.install(self.root)
        intermediate = Path(signing.__file__).resolve().parent / 'certificates/AppleWWDRCAG3.pem'
        certificate_der = base64.b64decode(''.join(intermediate.read_text().splitlines()[1:-1]))
        self.assertEqual(hashlib.sha256(certificate_der).hexdigest(),
                         'dcf21878c77f4198e4b4614f03d696d89c66c66008d4244e1b99161aac91601f')
        chain_import = ('security', 'import', str(intermediate), '-k', str(self.root / 'signing.keychain-db'))
        self.assertIn(chain_import, self.calls)
        identity_check = next(call for call in self.calls if call[1] == 'find-identity')
        self.assertLess(self.calls.index(chain_import), self.calls.index(identity_check))
        signing.cleanup(self.root)

    def test_rejects_wrong_expired_and_device_profiles_before_identity_import(self):
        for change in [
            {'ExpirationDate': datetime.datetime(2000, 1, 1)},
            {'ProvisionedDevices': ['device']},
            {'ProvisionsAllDevices': True},
            {'Entitlements': {'application-identifier': 'PREFIX123.other.app'}},
            {'Entitlements': {'application-identifier': 'PREFIX123.io.swps.nockster', 'get-task-allow': True}},
        ]:
            with self.subTest(change=change):
                with patch.dict(self.profile, change):
                    with self.assertRaises(ValueError):
                        signing.install(self.root)
                self.assertFalse(any(call[1] == 'import' for call in self.calls))
                signing.cleanup(self.root)
                self.assertFalse(self.root.exists())

    def test_profile_decode_failure_remains_cleanable(self):
        def fail_decode(*args):
            if args[1] == 'cms':
                raise subprocess.CalledProcessError(1, args)
            return self.command(*args)

        with patch.object(signing, 'run', side_effect=fail_decode):
            with self.assertRaises(subprocess.CalledProcessError):
                signing.install(self.root)
        signing.cleanup(self.root)
        self.assertFalse(self.root.exists())
        self.assertTrue(any(call[1] == 'delete-keychain' for call in self.calls))

    def test_identity_mismatch_remains_cleanable(self):
        self.profile['DeveloperCertificates'] = [b'another certificate']
        with self.assertRaisesRegex(ValueError, 'valid signing identity'):
            signing.install(self.root)
        signing.cleanup(self.root)
        self.assertFalse(self.root.exists())
        self.assertTrue(any(call[1] == 'delete-keychain' for call in self.calls))

    def test_cleanup_handles_an_early_failure(self):
        signing.cleanup(self.root)
        self.assertEqual(self.calls, [])


if __name__ == '__main__':
    unittest.main()
