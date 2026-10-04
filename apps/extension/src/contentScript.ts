const NOCKSTER_PROTOCOLS = new Set(['web+nockster:', 'nockster:']);

function sendOpenHandoffMessage(payload: {
  action: string;
  params: Array<[string, string]>;
  origin: string;
}): void {
  if (typeof chrome === 'undefined' || !chrome.runtime?.sendMessage) {
    return;
  }

  try {
    chrome.runtime.sendMessage({ type: 'openHandoff', ...payload }, () => {
      if (chrome.runtime.lastError) {
        console.warn(
          '[Nockster][contentScript] Failed to open handoff popup:',
          chrome.runtime.lastError
        );
      }
    });
  } catch (err) {
    console.warn('[Nockster][contentScript] Failed to send handoff message:', err);
  }
}

function parseHandoffLink(href: string): { action: string; params: URLSearchParams } | null {
  try {
    const url = new URL(href);
    if (!NOCKSTER_PROTOCOLS.has(url.protocol)) {
      return null;
    }

    const action = url.hostname || url.pathname.replace(/^\//, '');
    if (!action) {
      return null;
    }

    return { action, params: new URLSearchParams(url.search) };
  } catch {
    return null;
  }
}

function findAnchor(target: EventTarget | null): HTMLAnchorElement | null {
  if (!target || !(target instanceof Element)) {
    return null;
  }

  return target.closest('a[href]');
}

function handleClick(event: MouseEvent) {
  const anchor = findAnchor(event.target);
  if (!anchor) {
    return;
  }

  const href = anchor.getAttribute('href');
  if (!href) {
    return;
  }

  const parsed = parseHandoffLink(href);
  if (!parsed) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();

  // Brave (and some Chromium variants) block websites from navigating directly to `chrome-extension://...`.
  // Route the request through the extension service worker so it can open the UI safely.
  sendOpenHandoffMessage({
    action: parsed.action,
    params: Array.from(parsed.params.entries()),
    origin: window.location.origin
  });
}

document.addEventListener('click', handleClick, true);

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (sender.id === chrome.runtime.id && message?.type === 'website:validateDocument')
    respond({ origin: location.origin });
});

window.addEventListener('message', async event => {
  if (window !== window.top || event.source !== window || event.origin !== location.origin) return;
  const data = event.data;
  if (data?.target !== 'NOCKSTER_PROVIDER' || typeof data.id !== 'string' || data.id.length > 100)
    return;
  if (data.kind !== 'request' && data.kind !== 'cancel') return;
  try {
    const reply = await chrome.runtime.sendMessage({
      type: data.kind === 'cancel' ? 'website:cancel' : 'website:request',
      id: data.id,
      payload: data.payload
    });
    if (data.kind === 'request')
      window.postMessage(
        { target: 'NOCKSTER_PROVIDER', kind: 'response', id: data.id, ...reply },
        location.origin
      );
  } catch {
    window.postMessage(
      {
        target: 'NOCKSTER_PROVIDER',
        kind: 'response',
        id: data.id,
        error: { code: 4900, message: 'Nockster is unavailable. Refresh this page and try again.' }
      },
      location.origin
    );
  }
});
