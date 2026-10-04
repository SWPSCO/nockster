#!/usr/bin/env node

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Color mapping from hardcoded to CSS variables
// FIXED: Corrected color mappings for proper light/dark theme
const colorMap = {
  // Text colors (black in light mode)
  '#000000': 'var(--color-text)',
  '#000': 'var(--color-text)',
  '#333333': 'var(--color-text)',
  '#333': 'var(--color-text)',

  // Background colors (white in light mode)
  '#ffffff': 'var(--color-background)',
  '#fff': 'var(--color-background)',
  '#fafafa': 'var(--color-background)',

  // Secondary text
  '#666666': 'var(--color-text-secondary)',
  '#666': 'var(--color-text-secondary)',
  '#6b7280': 'var(--color-text-secondary)',

  // Tertiary text
  '#999999': 'var(--color-text-tertiary)',
  '#999': 'var(--color-text-tertiary)',
  '#9ca3af': 'var(--color-text-tertiary)',

  // Surface colors
  '#f3f4f6': 'var(--color-surface)',
  '#f9fafb': 'var(--color-surface)',
  '#f0f0f0': 'var(--color-surface)',

  // Border colors
  '#e5e7eb': 'var(--color-border)',
  '#d1d5db': 'var(--color-border)',
  '#e0e0e0': 'var(--color-border)',
  '#ddd': 'var(--color-border)',

  // Success colors
  '#16a34a': 'var(--color-success)',
  '#059669': 'var(--color-success)',
  '#10b981': 'var(--color-success)',

  // Error colors
  '#dc2626': 'var(--color-error)',
  '#ef4444': 'var(--color-error)',
  '#fecaca': 'var(--color-error)',
  '#fef2f2': 'var(--color-error-light)',

  // Warning colors
  '#f59e0b': 'var(--color-warning)',
  '#fbbf24': 'var(--color-warning)',

  // Special mappings for button states
  '#374151': 'var(--color-text-secondary)',
};

// Special context-aware replacements
const contextReplacements = [
  // Button primary background
  { pattern: /\.button--primary\s*\{[^}]*background:\s*#000;/g, replacement: '.button--primary {\n    background: var(--color-primary);' },
  { pattern: /\.button--primary\s*\{[^}]*color:\s*#ffffff;/g, replacement: '.button--primary {\n    color: var(--color-background);' },

  // Button secondary
  { pattern: /\.button--secondary\s*\{[^}]*background:\s*#ffffff;/g, replacement: '.button--secondary {\n    background: var(--color-background);' },
  { pattern: /\.button--secondary\s*\{[^}]*color:\s*#000;/g, replacement: '.button--secondary {\n    color: var(--color-text);' },
];

function processFile(filePath) {
  if (!filePath.endsWith('.svelte')) return;

  let content = fs.readFileSync(filePath, 'utf8');
  let modified = false;
  const originalContent = content;

  // Extract style section
  const styleMatch = content.match(/<style[^>]*>([\s\S]*?)<\/style>/);
  if (!styleMatch) return;

  let styleContent = styleMatch[1];
  const originalStyle = styleContent;

  // Sort color mappings by length (longer first) to avoid partial replacements
  const sortedColors = Object.keys(colorMap).sort((a, b) => b.length - a.length);

  // Replace colors in style properties
  sortedColors.forEach(color => {
    // Match color in CSS properties (background, color, border, etc.)
    const patterns = [
      new RegExp(`(background(?:-color)?\\s*:\\s*)${escapeRegExp(color)}(\\s*[;!])`, 'gi'),
      new RegExp(`(color\\s*:\\s*)${escapeRegExp(color)}(\\s*[;!])`, 'gi'),
      new RegExp(`(border(?:-color)?\\s*:\\s*[^;]*?)${escapeRegExp(color)}`, 'gi'),
      new RegExp(`(border-(?:top|bottom|left|right)(?:-color)?\\s*:\\s*[^;]*?)${escapeRegExp(color)}`, 'gi'),
      new RegExp(`(box-shadow\\s*:\\s*[^;]*?)${escapeRegExp(color)}`, 'gi'),
      new RegExp(`(outline(?:-color)?\\s*:\\s*)${escapeRegExp(color)}(\\s*[;!])`, 'gi'),
      new RegExp(`(fill\\s*:\\s*)${escapeRegExp(color)}(\\s*[;!])`, 'gi'),
      new RegExp(`(stroke\\s*:\\s*)${escapeRegExp(color)}(\\s*[;!])`, 'gi'),
    ];

    patterns.forEach(pattern => {
      const replacement = colorMap[color];
      styleContent = styleContent.replace(pattern, (match, prefix, suffix) => {
        modified = true;
        return prefix + replacement + (suffix || '');
      });
    });
  });

  // Apply context-aware replacements
  contextReplacements.forEach(({ pattern, replacement }) => {
    if (pattern.test(styleContent)) {
      styleContent = styleContent.replace(pattern, replacement);
      modified = true;
    }
  });

  if (modified) {
    // Replace the style content in the original file
    content = content.replace(/<style[^>]*>[\s\S]*?<\/style>/, `<style>${styleContent}</style>`);

    // Write the file
    fs.writeFileSync(filePath, content);
    console.log(`✅ Updated: ${path.basename(filePath)}`);

    // Count changes
    const changeCount = (originalStyle.match(/#[0-9a-fA-F]{3,6}/g) || []).length -
                       (styleContent.match(/#[0-9a-fA-F]{3,6}/g) || []).length;
    if (changeCount > 0) {
      console.log(`   Replaced ${changeCount} color values`);
    }
  }
}

function escapeRegExp(string) {
  return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function processDirectory(dirPath) {
  const files = fs.readdirSync(dirPath);

  files.forEach(file => {
    const fullPath = path.join(dirPath, file);
    const stat = fs.statSync(fullPath);

    if (stat.isDirectory()) {
      processDirectory(fullPath);
    } else {
      processFile(fullPath);
    }
  });
}

// Process all component directories
const componentsDir = path.join(__dirname, 'src/lib/components');

console.log('🎨 Starting dark mode color update...\n');
console.log('Processing components in:', componentsDir);
console.log('─'.repeat(50));

processDirectory(componentsDir);

console.log('─'.repeat(50));
console.log('\n✨ Dark mode color update complete!');
console.log('Next steps:');
console.log('1. Review the changes');
console.log('2. Test dark mode toggle in Settings');
console.log('3. Commit the changes');