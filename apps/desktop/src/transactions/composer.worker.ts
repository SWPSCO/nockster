import init, {
  compose_tx_v1_min_inputs,
  is_valid_pkh
} from '../../../../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm.js';
import { validateNotes, type ComposeInput, type ComposeSummary } from './composer';

self.onmessage = async (event: MessageEvent<ComposeInput>) => {
  try {
    await init();
    const input = event.data;
    const result = compose_tx_v1_min_inputs({
      ...input,
      notes: validateNotes(input.notes, is_valid_pkh)
    });
    try {
      const summary: ComposeSummary = JSON.parse(result.summary_json);
      self.postMessage({ bytes: result.wallet_jam, summary, txId: result.tx_id });
    } finally {
      result.free();
    }
  } catch (failure) {
    self.postMessage({ error: failure instanceof Error ? failure.message : String(failure) });
  }
};
