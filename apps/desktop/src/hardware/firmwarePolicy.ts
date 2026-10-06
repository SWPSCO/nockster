import { FEATURE_UPDATE_BOOT_STATUS, type SecurityStatus } from '@swps/nockster-js';

export const encryptedBootStatusRelease = 11;

export function canReadBootStatus(
  features: number,
  release: number | null,
  security: SecurityStatus | null
) {
  return (
    Boolean(features & FEATURE_UPDATE_BOOT_STATUS) &&
    (security?.flash_encryption === false ||
      (release !== null && release >= encryptedBootStatusRelease))
  );
}
