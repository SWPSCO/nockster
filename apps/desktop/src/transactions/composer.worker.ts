import init, {
  compose_tx_v1_min_inputs,
  is_valid_pkh
} from '../../../../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm.js';
import { largestNotesFirst, notesForAmount } from './note-selection';
import { validateNotes, type ComposeInput, type ComposeSummary } from './composer';

self.onmessage = async (event: MessageEvent<ComposeInput>) => {
  try {
    const input = event.data;
    const notes = largestNotesFirst(input.notes);
    const target = input.outputs.reduce((sum, output) => sum + BigInt(output.amount), 0n);
    if (notes.reduce((sum, note) => sum + BigInt(note.assets), 0n) <= target)
      throw new Error('Insufficient funds to cover requested outputs and fee.');
    await init();
    let count = notesForAmount(notes, target + 1n).length;
    while (count > 0) {
      try {
        const result = compose_tx_v1_min_inputs({
          ...input,
          notes: validateNotes(notes.slice(0, count), is_valid_pkh)
        });
        try {
          const bytes = result.wallet_jam;
          const summary: ComposeSummary = JSON.parse(result.summary_json);
          self.postMessage({ bytes, summary, txId: result.tx_id });
          return;
        } finally {
          result.free();
        }
      } catch (failure) {
        const message = failure instanceof Error ? failure.message : String(failure);
        if (!message.includes('insufficient funds') || count === notes.length) throw failure;
        count = Math.min(notes.length, Math.max(count + 1, count * 2));
      }
    }
    throw new Error('No spendable notes are available.');
  } catch (failure) {
    self.postMessage({ error: failure instanceof Error ? failure.message : String(failure) });
  }
};
