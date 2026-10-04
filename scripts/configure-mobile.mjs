import { readFileSync, writeFileSync } from 'node:fs';

// Use a deployment-target string so SwiftPM's manifest API does not constrain the OS target.
const packagePath = 'apps/mobile/ios/App/CapApp-SPM/Package.swift';
const source = readFileSync(packagePath, 'utf8');
writeFileSync(packagePath, source.replace(/\.iOS\(\.v26\)/g, '.iOS("26.0")'));
