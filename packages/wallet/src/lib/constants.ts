// Shared constants for Nockster Wallet

const DEFAULT_RPC_URL = import.meta.env.VITE_RPC_URL || 'https://nockblocks.com/rpc';

const deriveRpcV1Url = (url: string): string => {
  const trimmed = url.endsWith('/') ? url.slice(0, -1) : url;
  return trimmed.endsWith('/v1') ? trimmed : `${trimmed}/v1`;
};

// Network Configuration
export const NETWORK = {
  NAME: 'Nockchain',
  RPC_URL: DEFAULT_RPC_URL,
  RPC_V1_URL: import.meta.env.VITE_RPC_V1_URL || deriveRpcV1Url(DEFAULT_RPC_URL),
  EXPLORER_URL: 'https://explorer.nockchain.com',
  CHAIN_ID: 'nockchain-mainnet',
  DEFAULT_TIMEOUT: 30000,
  RETRY_ATTEMPTS: 3
} as const;

// Wallet Configuration
export const WALLET = {
  MAX_ADDRESSES: 256,
  DEFAULT_ADDRESS_COUNT: 10,
  MNEMONIC_WORDS: 24,
  AUTO_LOCK_TIMEOUT: 5 * 60 * 1000, // 5 minutes
  MAX_NAME_LENGTH: 20
} as const;

// Transaction Configuration
export const TRANSACTION = {
  MIN_AMOUNT: 1,
  MAX_AMOUNT: Number.MAX_SAFE_INTEGER,
  DEFAULT_FEE: 50,
  FEE_LEVELS: {
    FAST: 100,
    STANDARD: 50,
    ECONOMY: 25
  },
  CONFIRMATION_BLOCKS: 6
} as const;

// UI Configuration
export const UI = {
  ANIMATION_DURATION: 300,
  DEBOUNCE_DELAY: 500,
  TOAST_DURATION: 3000,
  MAX_ITEMS_PER_PAGE: 20,
  POPUP_WIDTH: 360,
  POPUP_HEIGHT: 600
} as const;

// Storage Keys
export const STORAGE_KEYS = {
  VAULT: 'vault',
  SETTINGS: 'settings',
  PERMISSIONS: 'permissions',
  ADDRESS_BOOK: 'addressBook',
  RECENT_TRANSACTIONS: 'recentTransactions',
  THEME: 'theme'
} as const;

// Error Messages
export const ERROR_MESSAGES = {
  NETWORK_ERROR: 'Network connection failed. Please try again.',
  REQUEST_TIMEOUT: 'Network request timed out. Please try again.',
  INVALID_ADDRESS: 'Invalid address format',
  INSUFFICIENT_BALANCE: 'Insufficient balance for this transaction',
  INVALID_MNEMONIC: 'Invalid recovery phrase',
  WALLET_LOCKED: 'Wallet is locked. Please unlock to continue.',
  TRANSACTION_FAILED: 'Transaction failed. Please try again.',
  UNKNOWN_ERROR: 'An unexpected error occurred'
} as const;

// Regex Patterns
export const PATTERNS = {
  ADDRESS: /^nock1[a-z0-9]{38}$/,
  AMOUNT: /^\d+(\.\d{1,8})?$/,
  NAME: /^[a-zA-Z0-9\s\-_]{1,20}$/
} as const;

// Routes
export const ROUTES = {
  WELCOME: 'welcome',
  DASHBOARD: 'dashboard',
  SEND: 'send',
  RECEIVE: 'receive',
  SETTINGS: 'settings',
  LOCK_SCREEN: 'lock-screen',
  IMPORT_WALLET: 'import-wallet',
  CREATE_WALLET: 'seed-phrase'
} as const;

// External Links
export const LINKS = {
  WEBSITE: 'https://nockster.com',
  DOCUMENTATION: 'https://nockster.com/docs',
  SUPPORT: 'https://nockster.com/support',
  GITHUB: 'https://github.com/SWPSCO/fletch',
  TERMS: 'https://nockster.com/terms',
  PRIVACY: 'https://nockster.com/wallet/privacy'
} as const;
