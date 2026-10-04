# Wallet CI cutover to SWPSCO/nockster

The wallet workflows run from the root of `SWPSCO/nockster`. Google Play uses a
dedicated workload identity provider whose trust condition must name this
repository and `.github/workflows/android.yml` on `master`.

## Repository settings

| Setting | Value |
| --- | --- |
| Destination repository | `SWPSCO/nockster` |
| Destination repository ID | `1045974658` |
| Organization ID | `186219242` |
| Release branch | `master` |
| Google project | `nockpool` (`402961206222`) |
| Identity pool / provider | `nockster-github` / `nockster-master` |
| Play service account | `nockster-play-ci@nockpool.iam.gserviceaccount.com` |
| Android application ID | `io.swps.nockster` |

Set the destination's non-secret Actions variables:

```sh
gh variable set GOOGLE_PLAY_WORKLOAD_IDENTITY_PROVIDER --repo SWPSCO/nockster \
  --body projects/402961206222/locations/global/workloadIdentityPools/nockster-github/providers/nockster-master
gh variable set GOOGLE_PLAY_SERVICE_ACCOUNT --repo SWPSCO/nockster \
  --body nockster-play-ci@nockpool.iam.gserviceaccount.com
```

Verify that repository or organization policies expose these secrets to
`SWPSCO/nockster`. GitHub does not return secret values for copying.

| Job | Required secrets |
| --- | --- |
| Android signing | `ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` |
| Wallet R2 downloads | `RCLONE_B64` |
| iOS signing | `IOS_DISTRIBUTION_P12_B64`, `IOS_DISTRIBUTION_P12_PASSWORD`, `IOS_PROVISIONING_PROFILE_B64` |
| App Store Connect / TestFlight | `AC_API_ISSUER_ID`, `AC_API_KEY_ID`, `AC_API_KEY_PATH` |
| macOS desktop signing | `APPLE_P12`, `APPLE_P12_PASSWORD`, plus the App Store Connect secrets above |
| Extension signing | `FLETCH_EXT_KEY_B64` |

Keep the same signing identities. The extension ID is
`nalokkijbnoknjoojndhgdlapkfjgamb`, and the desktop app identifier is
`io.swps.nockster.desktop`. Wallet downloads continue to use
`r2:swpsco/fletch/` and `https://bin.aeroe.io/fletch/`.

The iOS runner needs labels `self-hosted`, `macOS`, and `ARM64`, Xcode 26+, and
access to this repository. The destination has a registered `np-macmini` runner;
it must be online before dispatching iOS. Hardware jobs retain their own
runner and signing requirements. Do not disable Actions for the whole destination:
its hardware releases and Pages deployment use the same repository.

## Google identity cutover

These commands require an authenticated Google account with permission to
update the provider and the service account IAM policy. Authenticate with
`gcloud auth login` when the current credentials require reauthentication.

Let active wallet release runs finish, then disable the source wallet workflow
so the two repositories cannot publish competing builds:

```sh
gh workflow disable build.yml --repo SWPSCO/fletch
```

Grant the destination access to the existing Play service account:

```sh
gcloud iam service-accounts add-iam-policy-binding \
  nockster-play-ci@nockpool.iam.gserviceaccount.com \
  --project=nockpool --role=roles/iam.workloadIdentityUser \
  --member=principalSet://iam.googleapis.com/projects/402961206222/locations/global/workloadIdentityPools/nockster-github/attribute.repository_id/1045974658
```

Restrict the dedicated provider to the destination, release branch, and reusable
Android workflow. Its issuer, attribute mappings, and resource name remain in use:

```sh
gcloud iam workload-identity-pools providers update-oidc nockster-master \
  --project=nockpool --location=global \
  --workload-identity-pool=nockster-github \
  --attribute-condition="assertion.repository_owner_id == '186219242' && assertion.repository_id == '1045974658' && assertion.ref == 'refs/heads/master' && assertion.job_workflow_ref == 'SWPSCO/nockster/.github/workflows/android.yml@refs/heads/master' && assertion.event_name in ['push', 'workflow_dispatch']"
```

Remove the source repository's specific impersonation binding:

```sh
gcloud iam service-accounts remove-iam-policy-binding \
  nockster-play-ci@nockpool.iam.gserviceaccount.com \
  --project=nockpool --role=roles/iam.workloadIdentityUser \
  --member=principalSet://iam.googleapis.com/projects/402961206222/locations/global/workloadIdentityPools/nockster-github/attribute.repository_id/1048534824
```

Allow a few minutes for IAM propagation. The service account keeps its existing
Play Console permissions; there is no new Play app or JSON service-account key.
Both the Android caller and the Google Play job retain `id-token: write`.

## Build numbering and validation

`release-version.json` sets `buildSequenceOffset` to **1000**. The planner uses:

- `sequence = buildSequenceOffset + github.run_number`
- Android version code: `sequence * 100 + github.run_attempt`
- iOS build number: `sequence.github.run_attempt`
- Release tag: `vVERSION-build.SEQUENCE.ATTEMPT`

The first destination workflow run therefore produces Android **100101** and
iOS **1001.1**. The sequence must remain within 1–9999, and the attempt within
1–99; invalid values stop planning. Keep the offset fixed for subsequent runs.
The source's observed workflow counter is 91 at cutover preparation. Before
store publication, verify that the planned numbers exceed the highest builds
in Play Console and App Store Connect, including any uploaded outside CI.

Once the code is on `master` and the settings are ready, dispatch **Build and
release Nockster** with `platform=android` and all publication checkboxes clear.
Check the planned version code, signed APK/AAB, and R2 artifacts. Then dispatch
Android with `google_play` selected and confirm the release in Internal testing.
Verify iOS with `platform=ios` and `testflight` selected once its runner is online.

Master pushes build selected wallet platforms and update their R2 downloads.
Store uploads require the corresponding manual checkbox or `publish`.
Cross-repository Actions concurrency groups do not serialize publication;
keep the source wallet release workflow disabled after cutover.

See [Google Play setup](google-play.md) for the app's publishing permissions.
References: [Google federation claims](https://docs.cloud.google.com/iam/docs/workload-identity-federation-with-deployment-pipelines),
[GitHub reusable-workflow identity](https://docs.github.com/en/actions/how-tos/secure-your-work/security-harden-deployments/oidc-with-reusable-workflows).
