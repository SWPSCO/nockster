#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const mode = process.argv[2] || 'production';
const isDev = mode === 'development';

const baseManifest = {
    "manifest_version": 3,
    "version": process.env.NOCKSTER_VERSION || require('../release-version.json').version,
    "action": {
        "default_icon": {
            "16": "icons/icon-16.png",
            "48": "icons/icon-48.png",
            "128": "icons/icon-128.png"
        }
    },
    "icons": {
        "16": "icons/icon-16.png",
        "48": "icons/icon-48.png",
        "128": "icons/icon-128.png"
    },
    "permissions": ["storage"],
    "host_permissions": ["https://nockblocks.com/*"],
    "background": {
        "service_worker": "dist/background.js",
        "type": "module"
    },
    "content_scripts": [
        {
            "matches": ["http://*/*", "https://*/*"],
            "js": ["dist/inpage.js"],
            "world": "MAIN",
            "run_at": "document_start"
        },
        {
            "matches": [
                "<all_urls>"
            ],
            "js": [
                "dist/contentScript.js"
            ],
            "all_frames": true,
            "run_at": "document_start"
        }
    ]
};

// Apply environment-specific changes
if (isDev) {
    Object.assign(baseManifest, {
        "name": "Nockster - Nockchain Wallet (DEV)",
        "description": "Development version - A secure light wallet for the Nockchain network",
    });

    baseManifest.action = {
        ...baseManifest.action,
        "default_title": "Nockster Wallet (DEV)",
        "default_popup": "dev-popup.html"
    };

    baseManifest["content_security_policy"] = {
        "extension_pages": "script-src 'self' 'wasm-unsafe-eval' http://localhost:5173 http://localhost:5174 http://localhost:5175; object-src 'self'"
    };

    baseManifest["web_accessible_resources"] = [{
        "resources": ["*"],
        "matches": ["<all_urls>"]
    }];
} else {
    Object.assign(baseManifest, {
        "name": "Nockster - Nockchain Wallet",
        "description": "A secure light wallet for the Nockchain network",
    });

    baseManifest.action = {
        ...baseManifest.action,
        "default_title": "Nockster Wallet",
        "default_popup": "dist/index.html"
    };

    baseManifest["content_security_policy"] = {
        "extension_pages": "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'"
    };
}

// Write the manifest
const outputPath = path.join(__dirname, '..', 'apps', 'extension', 'ext', 'manifest.json');
fs.writeFileSync(outputPath, JSON.stringify(baseManifest, null, 4));

console.log(`✅ Generated manifest.json for ${mode} mode`);
