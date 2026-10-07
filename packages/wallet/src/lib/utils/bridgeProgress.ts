export const BRIDGE_CONFIRMATION_BLOCKS = 400;

export function bridgeProgress(
  inclusionHeight: number | null | undefined,
  tipHeight: number | null | undefined,
  status = 'confirmed'
) {
  const target = BRIDGE_CONFIRMATION_BLOCKS;
  if (status === 'failed')
    return { blocks: 0, target, phase: 'failed', label: 'Bridge deposit failed' };
  if (!Number.isSafeInteger(inclusionHeight) || inclusionHeight! < 0)
    return { blocks: 0, target, phase: 'inclusion', label: 'Waiting for inclusion in a block' };
  if (!Number.isSafeInteger(tipHeight) || tipHeight! < inclusionHeight!)
    return { blocks: null, target, phase: 'unavailable', label: 'Block count unavailable' };
  const blocks = Math.min(target, tipHeight! - inclusionHeight!);
  return {
    blocks,
    target,
    phase: blocks === target ? 'ready' : 'confirming',
    label:
      blocks === target ? '400 / 400 blocks · Block wait complete' : `${blocks} / ${target} blocks`
  };
}
