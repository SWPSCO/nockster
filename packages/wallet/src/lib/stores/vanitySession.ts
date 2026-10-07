import { get, writable } from 'svelte/store';
import {
  VanitySearch,
  initialVanityProgress,
  validateVanityOptions,
  type VanityOptions
} from '../services/vanity';

const state = writable({
  name: '',
  options: null as VanityOptions | null,
  progress: initialVanityProgress()
});
const search = new VanitySearch(progress => state.update(value => ({ ...value, progress })));

export const vanitySession = {
  subscribe: state.subscribe,
  start(name: string, input: VanityOptions) {
    const options = validateVanityOptions(input);
    search.start(options);
    state.update(value => ({ ...value, name, options }));
  },
  snapshot: () => get(state),
  take() {
    const candidate = search.take();
    state.update(value => ({ ...value, options: null, name: '' }));
    return candidate;
  },
  stop() {
    search.stop();
    state.update(value => ({ ...value, options: null, name: '' }));
  },
  clear() {
    search.clear();
    state.update(value => ({ ...value, options: null, name: '' }));
  }
};
