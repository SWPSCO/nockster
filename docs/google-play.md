# Google Play releases

For a GitHub repository rename, transfer, or copy, follow the
[CI repository move checklist](ci-repository-move.md).

Pushes to `master` build artifacts without uploading to mobile stores. A manual
**Build and release Nockster** run uploads the signed AAB to **Internal testing**
for `io.swps.nockster` when `android` or `all` is selected on `master` and either
`google_play` or `publish` is checked. The `publish` option also creates a GitHub
Release and uploads any selected iOS build to TestFlight.

The Android build saves the signed APK, AAB, R8 mapping file, and checksums as the
`release-android` Actions artifact and copies them to `r2:swpsco/fletch/`.
A separate Google Play job downloads that run's artifact and uploads the AAB,
which includes its mapping file. The uploader commits without the
`changesNotSentForReview` parameter. If Play explicitly requires manual review
submission, it retries that same edit once with `changesNotSentForReview=true`.
The bundle is uploaded only once. Other API errors fail the job.
Uploads are serialized for this package. A missing credential or rejected
upload fails the Google Play job; the build artifacts remain available.
The run summary records the requested destination and a successful upload.

## One-time Play Console setup

1. Open the `io.swps.nockster` app in Play Console. Create its first Internal
   testing release, enroll in Play App Signing, and upload the signed `.aab`
   from a successful `release-android` artifact or the R2 bucket. The package
   must have a manual upload before publishing through the API.
2. Complete the setup Play Console requires and roll out the initial Internal
   testing release. An app that remains in draft cannot accept CI's
   `completed` releases. Add a tester email list or Google Group and share the
   opt-in link with testers.
3. In a Google Cloud project, create a service account such as
   `nockster-play-ci`. Leave its project role selection empty. Publishing
   permissions come from Play Console.
4. In Play Console **Users and permissions**, invite the service account's
   email address. Give it app access to `io.swps.nockster` with **View app
   information (read-only)** and **Release apps to testing tracks**. Add
   **Manage testing tracks and edit tester lists** if the account also manages
   testers; this workflow only uploads releases.
5. Configure Workload Identity Federation as below. GitHub obtains short-lived
   credentials for this service account. No downloaded JSON key is needed;
   `iam.disableServiceAccountKeyCreation` can remain enforced.

## Keyless GitHub authentication

The person configuring Google Cloud needs permission to enable APIs, manage
workload identity pools, and edit this service account's IAM policy. The
corresponding roles are **Service Usage Admin**
(`roles/serviceusage.serviceUsageAdmin`), **Workload Identity Pool Admin**
(`roles/iam.workloadIdentityPoolAdmin`), and **Service Account Admin**
(`roles/iam.serviceAccountAdmin`). These are setup permissions for the human
administrator, not roles to grant to the publishing service account.

Run these commands in Google Cloud Shell or an authenticated `gcloud` session,
using the project ID and the existing service account email:

```sh
PLAY_PROJECT_ID='your-project-id'
PLAY_SERVICE_ACCOUNT='nockster-play-ci@your-project-id.iam.gserviceaccount.com'
PLAY_PROJECT_NUMBER=$(gcloud projects describe "$PLAY_PROJECT_ID" --format='value(projectNumber)')

gcloud services enable androidpublisher.googleapis.com iam.googleapis.com iamcredentials.googleapis.com sts.googleapis.com --project="$PLAY_PROJECT_ID"

gcloud iam workload-identity-pools create nockster-github \
  --project="$PLAY_PROJECT_ID" --location=global \
  --display-name='Nockster GitHub Actions'

gcloud iam workload-identity-pools providers create-oidc nockster-master \
  --project="$PLAY_PROJECT_ID" --location=global \
  --workload-identity-pool=nockster-github \
  --issuer-uri='https://token.actions.githubusercontent.com' \
  --attribute-mapping='google.subject=assertion.sub,attribute.repository_id=assertion.repository_id,attribute.repository_owner_id=assertion.repository_owner_id,attribute.ref=assertion.ref,attribute.job_workflow_ref=assertion.job_workflow_ref,attribute.event_name=assertion.event_name' \
  --attribute-condition="assertion.repository_owner_id == '186219242' && assertion.repository_id == '1045974658' && assertion.ref == 'refs/heads/master' && assertion.job_workflow_ref == 'SWPSCO/nockster/.github/workflows/android.yml@refs/heads/master' && assertion.event_name in ['push', 'workflow_dispatch']"

gcloud iam service-accounts add-iam-policy-binding "$PLAY_SERVICE_ACCOUNT" \
  --project="$PLAY_PROJECT_ID" \
  --role=roles/iam.workloadIdentityUser \
  --member="principalSet://iam.googleapis.com/projects/$PLAY_PROJECT_NUMBER/locations/global/workloadIdentityPools/nockster-github/attribute.repository_id/1045974658"

PLAY_PROVIDER="projects/$PLAY_PROJECT_NUMBER/locations/global/workloadIdentityPools/nockster-github/providers/nockster-master"
printf 'GOOGLE_PLAY_WORKLOAD_IDENTITY_PROVIDER=%s\nGOOGLE_PLAY_SERVICE_ACCOUNT=%s\n' "$PLAY_PROVIDER" "$PLAY_SERVICE_ACCOUNT"
```

The binding grants the GitHub identity permission to impersonate this one
service account. The provider restricts access to `SWPSCO/nockster`'s numeric
repository and organization IDs, `master`, the Android reusable workflow, and
push/manual release events. Forks and other branches do not match.

In GitHub **Settings → Secrets and variables → Actions → Variables**, add the
two printed values as repository variables. They are identifiers, not secrets.
If `gh` is available in the same shell, these commands set them:

```sh
gh variable set GOOGLE_PLAY_WORKLOAD_IDENTITY_PROVIDER --repo SWPSCO/nockster --body "$PLAY_PROVIDER"
gh variable set GOOGLE_PLAY_SERVICE_ACCOUNT --repo SWPSCO/nockster --body "$PLAY_SERVICE_ACCOUNT"
```

IAM changes can take a few minutes to propagate. The Google auth action obtains
a short-lived service account access token with the `androidpublisher` scope.
The uploader receives it through an environment variable; no credential file
or service account key is required.

Use a fresh workflow run after the manual upload. Android version codes are
`(buildSequenceOffset + run_number) * 100 + run_attempt`; the next upload must use an unused version
code. Re-running an older workflow can produce a lower version code than a
newer release. Keep the existing Android upload keystore and its four signing
secrets; the Google Play service account authorizes publishing and does not
replace the signing key.

## Availability and production

CI requests a completed Internal testing release. The run summary reports
whether Play accepts automatic review handling or requires manual submission
from **Publishing overview**. A successful CI upload does not mean the release
is approved or available to testers. Google can still require processing,
review, or app setup before testers receive it.

Commits use `changesInReviewBehavior=ERROR_IF_IN_REVIEW` to preserve an active
review. If Play reports changes already in review, wait for that review to
finish or manage it in Play Console before submitting another release.
Promote a tested bundle to production in Play Console when it is ready.

## DEX optimization

Release builds enable R8 obfuscation, code optimization, code shrinking, and
resource shrinking. Capacitor and AndroidX supply consumer keep rules for
their reflective entry points. Keep rules must target the required entry
points so the rest of the app remains eligible for optimization.

The AAB supplies the R8 mapping file to Play for readable crash reports, and CI
also retains a copy alongside the APK. CI checks the bundle's
`r8.json` metadata, requires at least 25% in each optimization category, and
verifies that the retained mapping matches the embedded mapping. The run
summary records the DEX size and optimization percentages.

Check the new bundle's DEX optimization percentages in App bundle explorer
after uploading it. Android
vitals findings identify a specific version code; a finding for an earlier
release does not describe a newly built bundle.

References: [Google Play API setup](https://developers.google.com/android-publisher/getting_started),
[Play Console permissions](https://support.google.com/googleplay/android-developer/answer/9844686),
[GitHub federation setup](https://github.com/google-github-actions/auth#workload-identity-federation-through-a-service-account),
[bundle uploads](https://developers.google.com/android-publisher/api-ref/rest/v3/edits.bundles/upload),
[Play edit review behavior](https://developers.google.com/android-publisher/api-ref/rest/v3/edits/commit),
[R8 release optimization](https://developer.android.com/topic/performance/app-optimization/enable-app-optimization).
