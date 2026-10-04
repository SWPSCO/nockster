#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

// File extensions to include
const INCLUDE_EXTENSIONS = [
  '.svelte',
  '.js',
  '.ts',
  '.rs',
  '.toml',
  '.json',
  '.html',
  '.css'
];

// Directories to search in
const INCLUDE_DIRS = [
  'packages/wallet/src',
  'apps/extension/src',
  'apps/desktop/src',
  'packages/wallet-engine/src',
  'apps/extension/ext' // Build output is excluded
];

// Files to specifically include
const SPECIFIC_FILES = [
  'package.json',
  'apps/extension/vite.config.ts',
  'apps/desktop/vite.config.ts',
  'apps/mobile/vite.config.ts',
  'svelte.config.js',
  'tsconfig.json',
  'eslint.config.js',
  '.prettierrc',
  'packages/wallet-engine/Cargo.toml'
];

// Patterns to exclude
const EXCLUDE_PATTERNS = [
  'node_modules',
  '.git',
  'dist',
  'build',
  'target',
  'design-reference',
  'examples/design-reference',
  '.DS_Store',
  'apps/extension/ext/dist',
  'apps/extension/ext/pkg',
  'scripts/concat-for-superai.cjs',
  'scripts/svg-to-png-icons.js',
  'nockster-codebase-superai.txt',
  'vite.config.js',
  'vite.config.dev.js'
];

// File size limit (350k chars, but leave some buffer)
const MAX_SIZE = 340000;

function shouldExclude(filePath) {
  for (const pattern of EXCLUDE_PATTERNS) {
    if (filePath.includes(pattern)) {
      return true;
    }
  }
  return false;
}

function shouldIncludeFile(filePath) {
  if (shouldExclude(filePath)) {
    return false;
  }

  // Check specific files
  for (const specific of SPECIFIC_FILES) {
    if (filePath === specific) {
      return true;
    }
  }

  // Check if in include directories
  let inIncludeDir = false;
  for (const dir of INCLUDE_DIRS) {
    if (filePath.startsWith(dir + '/') || filePath.startsWith('./' + dir + '/')) {
      inIncludeDir = true;
      break;
    }
  }

  if (!inIncludeDir) {
    return false;
  }

  // Check extension
  const ext = path.extname(filePath);
  return INCLUDE_EXTENSIONS.includes(ext);
}

function getAllFiles(dir, baseDir = dir, files = []) {
  if (shouldExclude(dir)) {
    return files;
  }

  let items;
  try {
    items = fs.readdirSync(dir);
  } catch (e) {
    return files;
  }

  for (const item of items) {
    const fullPath = path.join(dir, item);
    const relativePath = path.relative(baseDir, fullPath);

    if (shouldExclude(relativePath)) {
      continue;
    }

    let stat;
    try {
      stat = fs.statSync(fullPath);
    } catch (e) {
      continue;
    }

    if (stat.isDirectory()) {
      // Only recurse into directories we care about
      if (relativePath.startsWith('src') ||
          relativePath.startsWith('core') ||
          relativePath.startsWith('ext') ||
          relativePath === '.') {
        getAllFiles(fullPath, baseDir, files);
      }
    } else if (stat.isFile()) {
      if (shouldIncludeFile(relativePath)) {
        files.push({
          path: relativePath,
          fullPath: fullPath,
          size: stat.size
        });
      }
    }
  }

  return files;
}

function formatFileContent(file, content) {
  const separator = '='.repeat(80);
  const header = `
${separator}
FILE: ${file.path}
${separator}

`;

  return header + content + '\n\n';
}

function main() {
  const projectRoot = path.resolve(__dirname, '..');
  process.chdir(projectRoot);

  console.log('🔍 Scanning for relevant Nockster wallet source files...');

  // Get all files from the project
  let files = getAllFiles('.');

  // Also add specific files that might be missed
  for (const specific of SPECIFIC_FILES) {
    if (fs.existsSync(specific)) {
      const stat = fs.statSync(specific);
      const alreadyIncluded = files.some(f => f.path === specific);
      if (!alreadyIncluded) {
        files.push({
          path: specific,
          fullPath: path.resolve(specific),
          size: stat.size
        });
      }
    }
  }

  // Sort files by importance and type
  files.sort((a, b) => {
    // Priority order:
    // 1. Config files (package.json, etc)
    // 2. Main app files (App.svelte, main.js)
    // 3. Components
    // 4. Stores
    // 5. Other source files

    const getPriority = (path) => {
      if (path.includes('package.json')) return 0;
      if (path.includes('manifest.json')) return 1;
      if (path.includes('config')) return 2;
      if (path.includes('App.svelte')) return 3;
      if (path.includes('main.')) return 4;
      if (path.includes('/components/')) return 5;
      if (path.includes('/stores/')) return 6;
      if (path.includes('.svelte')) return 7;
      if (path.includes('packages/wallet-engine/src')) return 8;
      return 9;
    };

    const aPriority = getPriority(a.path);
    const bPriority = getPriority(b.path);

    if (aPriority !== bPriority) {
      return aPriority - bPriority;
    }

    return a.path.localeCompare(b.path);
  });

  console.log(`📁 Found ${files.length} relevant source files`);

  let output = `NOCKSTER WALLET COMPLETE SOURCE CODE
Generated: ${new Date().toISOString()}
Total Files: ${files.length}

This export contains all source code necessary for developing the Nockster wallet,
including Svelte components, stores, Rust/WASM code, and configuration files.

PROJECT STRUCTURE:
${files.map(f => `  - ${f.path} (${f.size} bytes)`).join('\n')}

${'='.repeat(80)}
SOURCE CODE:
${'='.repeat(80)}

`;

  let totalChars = output.length;
  let includedFiles = 0;
  let skippedFiles = [];

  for (const file of files) {
    try {
      const content = fs.readFileSync(file.fullPath, 'utf-8');
      const formatted = formatFileContent(file, content);

      if (totalChars + formatted.length > MAX_SIZE) {
        skippedFiles.push(file.path);
        continue;
      }

      output += formatted;
      totalChars += formatted.length;
      includedFiles++;

    } catch (error) {
      console.warn(`⚠️  Could not read ${file.path}: ${error.message}`);
    }
  }

  // Add summary at the end
  output += `
${'='.repeat(80)}
EXPORT SUMMARY:
${'='.repeat(80)}

Files Included: ${includedFiles}/${files.length}
Total Characters: ${totalChars}
Character Limit: ${MAX_SIZE}
Remaining Space: ${MAX_SIZE - totalChars}

${skippedFiles.length > 0 ? `\nFiles Skipped (size limit):\n${skippedFiles.map(f => `  - ${f}`).join('\n')}` : ''}

To use this file:
1. This contains the complete Nockster wallet source code
2. File paths are clearly marked for each section
3. Focus on Svelte components in packages/wallet/src/components and packages/wallet/src/lib/components
4. Wallet logic is in packages/wallet/src/stores/wallet.ts
5. WASM/Rust code is in packages/wallet-engine/src/
6. All code is TypeScript with strong type definitions in packages/wallet/src/lib/types
`;

  // Write the output
  const outputPath = 'nockster-codebase-superai.txt';
  fs.writeFileSync(outputPath, output);

  console.log(`✅ Successfully concatenated ${includedFiles} source files`);
  console.log(`📝 Output written to: ${outputPath}`);
  console.log(`📊 Total size: ${totalChars.toLocaleString()} characters (${((totalChars/MAX_SIZE)*100).toFixed(1)}% of limit)`);

  if (skippedFiles.length > 0) {
    console.log(`⚠️  ${skippedFiles.length} files skipped due to size limit`);
  }

  // Show breakdown by type
  const breakdown = {
    svelte: files.filter(f => f.path.endsWith('.svelte')).length,
    js: files.filter(f => f.path.endsWith('.js')).length,
    ts: files.filter(f => f.path.endsWith('.ts')).length,
    rust: files.filter(f => f.path.endsWith('.rs')).length,
    other: files.filter(f => !f.path.endsWith('.svelte') && !f.path.endsWith('.js') &&
                              !f.path.endsWith('.ts') && !f.path.endsWith('.rs')).length
  };

  console.log('\n📊 File breakdown:');
  console.log(`   Svelte components: ${breakdown.svelte}`);
  console.log(`   JavaScript files: ${breakdown.js}`);
  console.log(`   TypeScript files: ${breakdown.ts}`);
  console.log(`   Rust files: ${breakdown.rust}`);
  console.log(`   Config/Other: ${breakdown.other}`);
}

// Run the script
main();