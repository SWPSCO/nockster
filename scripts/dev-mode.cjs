const fs = require('fs');
const path = require('path');

const mode = process.argv[2] || 'dev';
const extDir = path.join(__dirname, '../apps/extension/ext');
const manifestPath = path.join(extDir, 'manifest.json');
const manifestDevPath = path.join(extDir, 'manifest.dev.json');
const manifestProdPath = path.join(extDir, 'manifest.prod.json');

// Backup original manifest if it doesn't exist
if (!fs.existsSync(manifestProdPath) && fs.existsSync(manifestPath)) {
    fs.copyFileSync(manifestPath, manifestProdPath);
    console.log('Backed up production manifest to manifest.prod.json');
}

if (mode === 'dev') {
    // Switch to dev manifest
    if (fs.existsSync(manifestDevPath)) {
        fs.copyFileSync(manifestDevPath, manifestPath);
        console.log('Switched to DEVELOPMENT mode');
        console.log('Now run: npm run dev');
        console.log('Then reload the extension in chrome://extensions');
    } else {
        console.error('Dev manifest not found!');
    }
} else if (mode === 'prod') {
    // Switch back to production manifest
    if (fs.existsSync(manifestProdPath)) {
        fs.copyFileSync(manifestProdPath, manifestPath);
        console.log('Switched to PRODUCTION mode');
        console.log('Now run: npm run build');
    } else {
        console.error('Production manifest not found!');
    }
} else {
    console.log('Usage: node scripts/dev-mode.cjs [dev|prod]');
}