import init, {
  createWallet,
  createWalletFromSeedPhrase,
  createTransaction
} from './pkg/nockster_core.js';

let wasmReady;

async function ensureWasm() {
  if (!wasmReady) {
    wasmReady = init();
  }
  try {
    await wasmReady;
  } catch (error) {
    console.error('WASM initialization failed:', error);
    throw error;
  }
}

export async function process(request) {
  await ensureWasm();

  switch (request.type) {
    case 'createWallet':
      try {
        return { success: true, wallet: createWallet() };
      } catch (e) {
        return { success: false, error: String(e) };
      }

    case 'createWalletFromSeedPhrase':
      try {
        return { success: true, wallet: createWalletFromSeedPhrase(request.seedPhrase) };
      } catch (e) {
        return { success: false, error: String(e) };
      }

    case 'createTransaction':
      try {
        return { success: true, transaction: createTransaction(request.notes) };
      } catch (e) {
        return { success: false, error: String(e) };
      }

    default:
      return { success: false, error: 'Unknown request type' };
  }
}
