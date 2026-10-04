import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import wasm from 'vite-plugin-wasm'
import topLevelAwait from 'vite-plugin-top-level-await'

const nockblocksRpcUrl = 'https://nockblocks.com/rpc'

function setJsonHeaders(res: any) {
  res.setHeader('content-type', 'application/json; charset=utf-8')
  res.setHeader('access-control-allow-origin', '*')
  res.setHeader('access-control-allow-methods', 'GET, POST, OPTIONS')
  res.setHeader('access-control-allow-headers', 'content-type, accept, authorization, x-api-key')
}

function headerValue(value: unknown): string | undefined {
  if (Array.isArray(value)) return value[0]
  return typeof value === 'string' ? value : undefined
}

function readRequestBody(req: any): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = []
    req.on('data', (chunk: Buffer | string) => {
      chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))
    })
    req.on('end', () => resolve(Buffer.concat(chunks)))
    req.on('error', reject)
  })
}

function nockblocksDevPlugin() {
  return {
    name: 'nockblocks-dev',
    configureServer(server: any) {
      server.middlewares.use('/__nockblocks/rpc', async (req: any, res: any, next: any) => {
        if (req.method === 'OPTIONS') {
          res.statusCode = 204
          setJsonHeaders(res)
          res.end()
          return
        }
        if (req.method !== 'POST') {
          next()
          return
        }

        try {
          const body = await readRequestBody(req)
          const headers: Record<string, string> = {
            accept: 'application/json',
            'content-type': headerValue(req.headers['content-type']) || 'application/json',
          }
          const authorization = headerValue(req.headers.authorization)
          const apiKey = headerValue(req.headers['x-api-key'])
          if (authorization) headers.authorization = authorization
          if (apiKey) headers['x-api-key'] = apiKey

          const upstream = await fetch(nockblocksRpcUrl, {
            method: 'POST',
            headers,
            body,
          })
          const text = await upstream.text()
          res.statusCode = upstream.status
          setJsonHeaders(res)
          res.end(text)
        } catch (error: any) {
          res.statusCode = 502
          setJsonHeaders(res)
          res.end(JSON.stringify({
            error: {
              message: error?.message ?? String(error),
              upstream: nockblocksRpcUrl,
            },
          }))
        }
      })
    },
  }
}

export default defineConfig({
  plugins: [
    react(),
    wasm(),
    topLevelAwait(),
    nockblocksDevPlugin(),
  ],
  server: {
    port: 3000,
    open: true,
    fs: {
      allow: ['..']  // Allow serving files from parent directory (for WASM pkg)
    }
  },
})
