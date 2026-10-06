import type { MineOptions, MineProgress, MineResult } from '../../../public/vanity/miner.js';

export type RecoveryKind = 'mnemonic' | 'raw';
export type VanityOptions = Pick<
  MineOptions,
  'prefix' | 'insensitive' | 'keyMode' | 'backend' | 'lanes' | 'steps' | 'maxAttempts'
>;
export type WalletCandidate = { kind: RecoveryKind | 'extended'; key: string; address?: string };
export type VanityProgress = {
  status: 'idle' | 'mining' | 'found' | 'stopped' | 'exhausted' | 'error';
  message: string;
  attempts: number;
  rate: number;
  seconds: number;
  backend: string;
  address?: string;
};

export const initialVanityProgress = (): VanityProgress => ({
  status: 'idle',
  message: '',
  attempts: 0,
  rate: 0,
  seconds: 0,
  backend: ''
});

export function validateVanityOptions(options: VanityOptions): VanityOptions {
  const prefix = options.prefix.trim();
  if (!prefix || prefix.length > 55) throw new Error('Enter an address prefix of 1–55 characters.');
  const alphabet = options.insensitive ? /^[a-zA-Z0-9]+$/ : /^[1-9A-HJ-NP-Za-km-z]+$/;
  if (!alphabet.test(prefix))
    throw new Error('Use Base58 characters, or enable letter equivalents for 0, O, I, and l.');
  const keyMode = options.keyMode ?? 'mnemonic';
  if (keyMode !== 'mnemonic' && keyMode !== 'raw') throw new Error('Choose a recovery method.');
  const backend = options.backend ?? 'auto';
  if (backend !== 'auto' && backend !== 'cpu') throw new Error('Choose Automatic or CPU.');
  const lanes = options.lanes ?? (keyMode === 'raw' ? 64 : 4096);
  const steps = options.steps ?? 1;
  const maxAttempts = options.maxAttempts ?? 0;
  if (!Number.isInteger(lanes) || lanes < 1 || lanes > (keyMode === 'raw' ? 256 : 4096))
    throw new Error('GPU lanes are outside the selected recovery method’s range.');
  if (!Number.isInteger(steps) || steps < 1 || steps > 16)
    throw new Error('Use 1–16 raw-key steps per batch.');
  if (!Number.isSafeInteger(maxAttempts) || maxAttempts < 0)
    throw new Error('Attempt limit must be a nonnegative whole number.');
  return {
    prefix,
    insensitive: !!options.insensitive,
    keyMode,
    backend,
    lanes,
    steps,
    maxAttempts
  };
}

let library:
  Promise<{ mineAddress: (options: MineOptions) => Promise<MineResult | null> }> | undefined;
function loadLibrary() {
  const url = new URL('vanity/miner.js', document.baseURI).href;
  return (library ??= import(/* @vite-ignore */ url).catch(error => {
    library = undefined;
    throw error;
  }));
}

/** One transient search. Only take() returns recovery material; status is public. */
export class VanitySearch {
  private controller: AbortController | null = null;
  private candidate: WalletCandidate | null = null;
  private generation = 0;
  private progress = initialVanityProgress();

  constructor(private changed: (progress: VanityProgress) => void = () => {}) {}

  snapshot(): VanityProgress {
    return { ...this.progress };
  }

  private update(patch: Partial<VanityProgress>) {
    this.progress = { ...this.progress, ...patch };
    this.changed(this.snapshot());
  }

  start(input: VanityOptions): void {
    const options = validateVanityOptions(input);
    this.clear();
    const started = performance.now();
    const generation = this.generation;
    const controller = new AbortController();
    this.controller = controller;
    this.update({
      ...initialVanityProgress(),
      status: 'mining',
      message: 'Starting local search…'
    });
    const onProgress = (event: MineProgress) => {
      if (this.generation !== generation) return;
      if (event.type === 'status') this.update({ message: event.message });
      if (event.type === 'backend')
        this.update({ backend: event.adapter, message: 'Searching for your address…' });
      if (event.type === 'progress')
        this.update({
          backend: event.adapter,
          attempts: event.attempts,
          seconds: event.seconds,
          rate: event.rate,
          message: 'Searching for your address…'
        });
    };
    void (async () => {
      let found: MineResult | null = null;
      try {
        const { mineAddress } = await loadLibrary();
        if (this.generation !== generation) return;
        found = await mineAddress({ ...options, signal: controller.signal, onProgress });
        if (this.generation !== generation) return;
        if (!found) {
          this.update({
            status: 'exhausted',
            message: 'Attempt limit reached. Try again or shorten the prefix.'
          });
          return;
        }
        const data = JSON.parse(new TextDecoder().decode(found.keyJson));
        const key = options.keyMode === 'raw' ? data.secret_key_hex_be : data.mnemonic;
        if (
          typeof key !== 'string' ||
          data.pkh !== found.pkh ||
          (options.keyMode === 'mnemonic' &&
            (data.derivation_path !== 'm' ||
              data.passphrase !== '' ||
              key.split(' ').length !== 24))
        ) {
          throw new Error('The generator returned invalid recovery material.');
        }
        this.candidate = { kind: options.keyMode!, key, address: found.pkh };
        const seconds = (performance.now() - started) / 1000;
        this.update({
          status: 'found',
          message: 'Matching address found',
          address: found.pkh,
          backend: found.backend,
          attempts: found.attempts,
          seconds,
          rate: seconds > 0 ? found.attempts / seconds : 0
        });
      } catch (error) {
        if (this.generation === generation)
          this.update({
            status: 'error',
            message: error instanceof Error ? error.message : 'Unable to generate an address.'
          });
      } finally {
        found?.keyJson.fill(0);
        if (this.generation === generation) this.controller = null;
      }
    })();
  }

  take(): WalletCandidate {
    if (!this.candidate || this.progress.status !== 'found')
      throw new Error('Find an address first.');
    const candidate = this.candidate;
    this.candidate = null;
    this.clear();
    return candidate;
  }

  stop(): void {
    this.clear();
    this.update({ status: 'stopped', message: 'Stopped. A new search uses fresh random keys.' });
  }

  clear(): void {
    this.generation++;
    this.controller?.abort();
    this.controller = null;
    if (this.candidate) this.candidate.key = '';
    this.candidate = null;
    this.update(initialVanityProgress());
  }
}
