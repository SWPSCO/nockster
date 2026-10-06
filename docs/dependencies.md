# JavaScript dependencies

Use Node.js 22.22.2+ on the 22.x line and npm 12.1.0. Both the wallet and
`examples/design-reference` declare their Node/npm requirements and commit their lockfiles.
CI installs npm 12.1.0 through `.github/actions/setup-js/action.yml` before
running `npm ci`.

The pinned npm release is a reviewed security exception to the age limit:
[npm 12.1.0](https://github.com/npm/cli/releases/tag/v12.1.0) contains patched
versions of its bundled `tar`, `undici`, `ip-address`, and `brace-expansion`.

```sh
npm install --global npm@12.1.0 --ignore-scripts
npm ci
npm run audit:deps
npm audit signatures
```

## Update policy

Each project's `.npmrc` applies these settings:

- `min-release-age=14`: new resolutions must be at least fourteen days old.
- `save-exact=true`: direct dependency versions are exact, including wallet SDKs.
- `ignore-scripts=true`: dependency installation does not execute lifecycle scripts.
- `engine-strict=true`: installations fail with an unsupported Node or npm version.

`npm ci` installs the reviewed lockfile, including already-locked releases; the
age limit governs resolving updates. Commit `package.json` and `package-lock.json`
together. The `iris-v1` alias names the pinned Iris SDK used for wallet auth.

Dependabot checks npm dependencies in both projects weekly and waits fourteen
days for routine version updates. Vite, its Svelte plugin, and Svelte update as
one group, including major versions, so their peer requirements can resolve
together. Capacitor and TypeScript ESLint packages are grouped for minor/patch
updates. TypeScript compiler majors require a coordinated manual upgrade with
Svelte and TypeScript ESLint; routine compiler minor/patch updates remain enabled.
Other major upgrades use separate PRs.
GitHub Actions updates have the same routine cooldown and retain commit-SHA pins.
Updates require review; auto-merge is not configured.

Dependabot alerts and security-update PRs identify known vulnerabilities.
Security PRs do not wait for the routine cooldown. If npm's age limit prevents
resolving a reviewed security patch, use a one-command exception for that
specific package:

```sh
npm install PACKAGE@PATCHED_VERSION --min-release-age-exclude=PACKAGE
```

For a transitive fix, use `npm audit fix --min-release-age-exclude=PACKAGE` or
edit its scoped override and regenerate the lockfile with that exception.
Review the advisory, package provenance, and lockfile diff before merging.
An exception does not enable install scripts or exempt the package's dependencies.
Do not add a persistent wildcard exclusion or disable the age limit globally.

## Build tools and validation

The `xcode` dependency uses `uuid` 11.1.1 through a scoped override. This fixes
[GHSA-w5hq-g745-h8pq](https://github.com/uuidjs/uuid/security/advisories/GHSA-w5hq-g745-h8pq)
while retaining the CommonJS `v4()` API used by Xcode project generation.
`tests/dependencies.test.ts` checks editing and serializing a real app project.

OpenAPI generation uses the locally installed, locked
`@openapitools/openapi-generator-cli`. Its Java generator version is declared in
`openapitools.json`. Icon generation uses the locked `sharp` dependency.
Dependency install scripts remain disabled for these tools, esbuild, and
optional platform packages; explicit project build scripts still run.

The shared engine CI job checks both lockfiles with `npm run audit:deps` and
verifies installed npm registry signatures before building. Any reported
vulnerability fails that audit step. Wallet tests, extension tests, and native
bridge tests validate behavior before signing. Release workflows retain manual
dispatch and `master` push triggers.

For dependency changes, run:

```sh
npm ci
npm run audit:deps
npm audit signatures
npm run typecheck
npm test
npm run build
npm run mobile:sync
npm run test:extension
npm run test:mobile
npm ci --prefix examples/design-reference
npm run build --prefix examples/design-reference
```

Registry signatures verify published package identity and integrity; they do
not establish that the publisher's code is safe. Advisory databases identify
known issues. The age limit and disabled install scripts reduce exposure while
review and behavioral checks remain necessary.

References: [npm release-age configuration](https://docs.npmjs.com/cli/v12/using-npm/config/#min-release-age),
[Dependabot cooldown](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference#cooldown).
