export function noteNameToId(note: unknown): string | null {
  if (!note || typeof note !== 'object') return null;
  const record = note as Record<string, unknown>;

  const name = record.name;
  if (name && typeof name === 'object') {
    const nameRecord = name as Record<string, unknown>;
    const firstName = nameRecord.firstName;
    const lastName = nameRecord.lastName;
    if (typeof firstName === 'string' && typeof lastName === 'string') {
      return `${firstName}/${lastName}`;
    }
  }

  const firstName = record.firstName;
  const lastName = record.lastName;
  if (typeof firstName === 'string' && typeof lastName === 'string') {
    return `${firstName}/${lastName}`;
  }

  return null;
}

export function collectReservedNoteIds(
  pendingTransactions: Array<{ inputNotes?: string[] }> | undefined
): Set<string> {
  const out = new Set<string>();
  for (const pendingTx of pendingTransactions ?? []) {
    for (const noteId of pendingTx.inputNotes ?? []) {
      out.add(noteId);
    }
  }
  return out;
}
