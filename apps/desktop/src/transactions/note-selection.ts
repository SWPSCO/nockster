import { noteId, type Note } from './composer.ts';

export function largestNotesFirst(notes: Note[]): Note[] {
  return [...notes].sort(
    (a, b) =>
      b.assets - a.assets || a.origin_page - b.origin_page || noteId(a).localeCompare(noteId(b))
  );
}

// An amount-only lower bound; the transaction engine adds inputs for the fee.
export function notesForAmount(notes: Note[], amount: bigint): Note[] {
  if (amount <= 0n) return [];
  let total = 0n;
  let count = 0;
  while (count < notes.length && total < amount) total += BigInt(notes[count++].assets);
  return notes.slice(0, count);
}
