let ready:
  | Promise<typeof import('../../../../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm.js')>
  | undefined;
export function hardwareCrypto() {
  return (ready ??= import('../../../../nockster-esp/crates/nockster-wasm/pkg/nockster_wasm.js')
    .then(async wasm => {
      await wasm.default();
      return wasm;
    })
    .catch(error => {
      ready = undefined;
      throw error;
    }));
}

export function download(bytes: Uint8Array, filename: string, type = 'application/octet-stream') {
  const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type }));
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
