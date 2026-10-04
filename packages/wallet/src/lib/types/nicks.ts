// Nicks type definition
// 1 NOCK = 65,536 nicks (2^16)
// Nicks are the smallest unit in the Nockpool system (like satoshis for Bitcoin)

/**
 * Nicks - the smallest unit in the Nockpool system
 * All internal monetary values should use this type for precision
 */
export type Nicks = bigint;

/**
 * Conversion constant: 1 NOCK = 2^16 nicks
 */
export const NICKS_PER_NOCK = 65536n;

/**
 * Protocol minimum fee floor; transaction size determines the actual fee.
 */
export const STANDARD_NETWORK_FEE = 256n;
