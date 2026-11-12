# Patched deepmerge for build tooling

Based on the MIT-licensed `deepmerge@4.3.1` npm distribution. `index.cjs` is its
bundled `dist/cjs.js`, with one change to `getKeys`: exclude `__proto__`,
`constructor`, and `prototype` from both target copying and source merging.
Ordinary configuration keys, symbols, arrays, custom merges, and cloning retain
upstream behavior. Prototype-control keys are intentionally discarded.

This fixes [GHSA-gcqx-w48m-w2h4](https://github.com/advisories/GHSA-gcqx-w48m-w2h4),
which has no patched upstream release as of 2026-11-12. The original target-copy
loop could invoke the `__proto__` setter on its new destination. Source validation
also depended on the target's prototype and could miss the same key.

The root and design-reference npm overrides route Svelte's `deepmerge` dependency
to this private local package. No install hooks or runtime patching are required.
The strict low-severity audit and registry-signature checks remain enabled.
The local fork is reviewed source, not a registry-signed third-party release.

`tests/deepmerge.test.ts` resolves the package through the installed Svelte plugin
and exercises the original exploit, nested/array/merge.all paths, and Svelte's
array-replacement behavior. Remove this fork and both overrides when an upstream
release fixes the flaw and passes those tests.

Original dist/cjs.js SHA-256: `2e5160f84a1141bd927264afe4d15766f1e6cf07daef182b37ff53fea121de7d`
