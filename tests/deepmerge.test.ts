import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';

// Resolve the dependency used by the installed Svelte plugin, not a test-only copy.
const require = createRequire(import.meta.resolve('@sveltejs/vite-plugin-svelte'));
const merge = require('deepmerge');
const payload = () => JSON.parse('{"__proto__":{"poisoned":true},"safe":1}');

function clean(value: object) {
  assert.equal(Object.getPrototypeOf(value), Object.prototype);
  assert.equal((value as any).poisoned, undefined);
  assert.equal(Object.hasOwn(value, '__proto__'), false);
  assert.equal(({} as any).poisoned, undefined);
}

test('merging either target or source cannot poison the returned object prototype', () => {
  for (const [target, source] of [
    [payload(), {}],
    [{}, payload()],
    [payload(), payload()],
    [Object.assign(Object.create(null), { safe: 1 }), payload()]
  ]) {
    const result = merge(target, source);
    clean(result);
    assert.equal(result.safe, 1);
  }
});

test('nested objects, array cloning, and merge.all reject prototype-control keys', () => {
  const hostile = JSON.parse('{"__proto__":{"poisoned":true},"constructor":{"prototype":{"poisoned":true}},"prototype":{"poisoned":true},"safe":1}');
  for (const result of [
    merge({ nested: hostile }, {}).nested,
    merge({}, { nested: hostile }).nested,
    merge([hostile], [])[0],
    merge.all([hostile, { extra: 2 }])
  ]) {
    clean(result);
    assert.equal(Object.hasOwn(result, 'constructor'), false);
    assert.equal(Object.hasOwn(result, 'prototype'), false);
    assert.equal(result.safe, 1);
  }
});

test('Svelte-style merging preserves compiler settings, functions, symbols and array replacement', () => {
  const preprocess = () => 'preprocess';
  const symbol = Symbol('config');
  const initial = { compilerOptions: { dev: true }, preprocess: [preprocess], [symbol]: { enabled: true } };
  const result = merge(initial, { compilerOptions: { css: 'external' }, preprocess: [] }, {
    arrayMerge: (_target: unknown[], source: unknown[]) => source
  });
  assert.deepEqual(result.compilerOptions, { dev: true, css: 'external' });
  assert.deepEqual(result.preprocess, []);
  assert.deepEqual(result[symbol], { enabled: true });
  assert.deepEqual(initial.preprocess, [preprocess]);
  assert.equal(merge({}, { preprocess }).preprocess, preprocess);
});
