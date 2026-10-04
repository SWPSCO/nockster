// Handle hot reload messages
const iframe = document.getElementById('app-frame');

// Propagate query params into the iframe so deep links work in dev mode.
if (iframe && iframe.src) {
    try {
        const url = new URL(iframe.src);
        url.search = window.location.search;
        iframe.src = url.toString();
    } catch (err) {
        console.warn('Failed to update dev iframe URL:', err);
    }
}

// Listen for errors and reload
iframe.addEventListener('error', () => {
    console.log('Dev server not running, trying to connect...');
    setTimeout(() => {
        iframe.src = iframe.src;
    }, 1000);
});
