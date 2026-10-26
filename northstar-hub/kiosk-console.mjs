// Reads the headset kiosk page's console and evaluates an expression through the Chrome DevTools
// Protocol (the hub starts the kiosk with --remote-debugging-port=9222).
//   node northstar-hub/kiosk-console.mjs "<js expression>" [waitMs]     RELOAD=1 reloads the page first
const [,, expr = 'document.title', waitMs = '6000'] = process.argv
const pages = await (await fetch('http://localhost:9222/json')).json()
const page = pages.find((p) => p.type === 'page' && p.url.includes('localhost:5173'))
if (!page) { console.log('no kiosk page; targets:', pages.map((p) => p.url)); process.exit(1) }
const ws = new WebSocket(page.webSocketDebuggerUrl)
let id = 0
const pending = new Map()
const send = (method, params = {}) => new Promise((resolve) => { pending.set(++id, resolve); ws.send(JSON.stringify({ id, method, params })) })
const logs = []
ws.addEventListener('message', ({ data: raw }) => {
  const m = JSON.parse(raw)
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result ?? m.error); pending.delete(m.id); return }
  if (m.method === 'Runtime.consoleAPICalled') logs.push(`[${m.params.type}] ` + m.params.args.map((a) => a.value ?? a.description ?? '').join(' '))
  if (m.method === 'Runtime.exceptionThrown') logs.push('[exception] ' + (m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text))
  if (m.method === 'Log.entryAdded') logs.push(`[log:${m.params.entry.level}] ${m.params.entry.text}`)
})
await new Promise((r) => ws.addEventListener('open', r))
await send('Runtime.enable'); await send('Log.enable'); await send('Page.enable')
if (process.env.RELOAD === '1') await send('Page.reload', { ignoreCache: false })
await new Promise((r) => setTimeout(r, Number(waitMs)))
const result = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })
console.log('url:', page.url)
console.log('eval:', JSON.stringify(result.result?.value ?? result))
console.log('--- console (' + logs.length + ')')
for (const l of logs.slice(-40)) console.log(l)
ws.close()
