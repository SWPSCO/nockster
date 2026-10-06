import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { createServer, type AddressInfo, type Socket } from 'node:net';

const require = createRequire(import.meta.url);

test('get-uri downloads through the patched FTP dependency', { timeout: 5000 }, async () => {
  const { getUri } = await import('get-uri');
  const payload = 'function FindProxyForURL() { return "DIRECT"; }';
  const sockets = new Set<Socket>();
  let dataSocket: Socket | undefined;
  const dataServer = createServer(socket => {
    dataSocket = socket;
    sockets.add(socket);
  });
  const controlServer = createServer(socket => {
    sockets.add(socket);
    socket.setEncoding('utf8');
    socket.write('220 Ready\r\n');
    let buffer = '';
    socket.on('data', chunk => {
      buffer += chunk;
      let boundary: number;
      while ((boundary = buffer.indexOf('\r\n')) !== -1) {
        const command = buffer.slice(0, boundary).split(' ')[0];
        buffer = buffer.slice(boundary + 2);
        switch (command) {
          case 'USER':
            socket.write('230 Logged in\r\n');
            break;
          case 'FEAT':
            socket.write('502 Unsupported\r\n');
            break;
          case 'MDTM':
            socket.write('213 20260101000000\r\n');
            break;
          case 'EPSV':
            socket.write(`229 Passive (|||${(dataServer.address() as AddressInfo).port}|)\r\n`);
            break;
          case 'RETR':
            socket.write('150 Opening data connection\r\n');
            dataSocket!.end(payload, () => socket.write('226 Transfer complete\r\n'));
            break;
          case 'QUIT':
            socket.end('221 Goodbye\r\n');
            break;
          default:
            socket.write('200 OK\r\n');
        }
      }
    });
  });
  try {
    dataServer.listen(0, '127.0.0.1');
    await once(dataServer, 'listening');
    controlServer.listen(0, '127.0.0.1');
    await once(controlServer, 'listening');
    const port = (controlServer.address() as AddressInfo).port;
    const stream = await getUri(`ftp://127.0.0.1:${port}/proxy.pac`);
    let content = '';
    for await (const chunk of stream) content += chunk.toString();
    assert.equal(content, payload);
  } finally {
    for (const socket of sockets) socket.destroy();
    await Promise.all(
      [dataServer, controlServer].map(
        server => new Promise<void>(resolve => server.close(() => resolve()))
      )
    );
  }
});

test('Xcode project edits round-trip with the patched UUID dependency', () => {
  const xcode = require('xcode');
  const directory = mkdtempSync(join(tmpdir(), 'nockster-xcode-'));
  try {
    const path = join(directory, 'project.pbxproj');
    writeFileSync(path, readFileSync('apps/mobile/ios/App/App.xcodeproj/project.pbxproj'));
    const project = xcode.project(path).parseSync();
    const group = project.addPbxGroup([], 'DependencyValidation');
    assert.match(group.uuid, /^[A-F0-9]{24}$/);
    writeFileSync(path, project.writeSync());
    const parsed = xcode.project(path).parseSync();
    assert.equal(parsed.hash.project.objects.PBXGroup[group.uuid].name, 'DependencyValidation');
    assert.deepEqual(parsed.pbxGroupByName('DependencyValidation').children, []);
    assert.equal(parsed.getFirstTarget().uuid, project.getFirstTarget().uuid);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('concurrently quotes arguments and rejects line terminators after comments', () => {
  const dependency = createRequire(require.resolve('concurrently'));
  const { quote, parse } = dependency('shell-quote');
  const args = ['echo', 'a b', "single'quote", 'double"quote', '$literal;value'];
  assert.deepEqual(parse(quote(args)), args);
  for (const separator of ['\n', '\r', '\u2028', '\u2029']) {
    assert.throws(
      () => quote(['echo', 'ok', { comment: 'note' }, `a${separator}echo injected`]),
      TypeError
    );
  }
});

test('indexed source maps preserve mappings and reject excessive section offsets', () => {
  const { SourceMapConsumer, SourceNode } = require('source-map-js');
  const map = {
    version: 3,
    sources: ['input.js'],
    names: [],
    mappings: 'AAAA',
    sourcesContent: ['x']
  };
  const indexed = (line: number) => ({
    version: 3,
    sections: [{ offset: { line, column: 0 }, map }]
  });
  const consumer = new SourceMapConsumer(indexed(1));
  assert.equal(SourceNode.fromStringWithSourceMap('\nx', consumer).toString(), '\nx');
  for (const offset of [1e12, Infinity, -1, 1.5]) {
    assert.throws(() => new SourceMapConsumer(indexed(offset)), /Section offset/);
  }
});

test('sharp renders SVG icons with its bundled image libraries', async () => {
  const sharp = require('sharp');
  const svg = Buffer.from(
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#fff"/></svg>'
  );
  const png = await sharp(svg).resize(32, 32).png().toBuffer();
  const metadata = await sharp(png).metadata();
  assert.equal(metadata.format, 'png');
  assert.equal(metadata.width, 32);
  assert.equal(metadata.height, 32);
});
