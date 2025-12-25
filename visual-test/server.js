/**
 * XR50 WebSocket Bridge
 * Pipes pose data from xvisio_test to connected WebSocket clients.
 * 
 * Usage: sudo ../libxvisio/build/xvisio_test | node server.js
 */

import { WebSocketServer } from 'ws';
import { createInterface } from 'readline';
import { createServer } from 'http';
import { readFileSync } from 'fs';

const PORT = 8080;
const clients = new Set();

// HTTP server for static files
const http = createServer((req, res) => {
    const file = req.url === '/' ? '/index.html' : req.url;
    const ext = file.split('.').pop();
    const types = { html: 'text/html', css: 'text/css', js: 'application/javascript' };
    
    let content;
    try {
        content = readFileSync('.' + file);
    } catch {
        res.writeHead(404);
        res.end('Not found');
        return;
    }
    res.writeHead(200, { 'Content-Type': types[ext] || 'text/plain' });
    res.end(content);
});

// WebSocket server
const wss = new WebSocketServer({ server: http });

wss.on('connection', (ws) => {
    clients.add(ws);
    console.log(`Client connected (${clients.size} total)`);
    ws.on('close', () => {
        clients.delete(ws);
        console.log(`Client disconnected (${clients.size} total)`);
    });
});

// Read pose data from stdin and broadcast
const rl = createInterface({ input: process.stdin });

rl.on('line', (line) => {
    clients.forEach((ws) => {
        if (ws.readyState === 1) ws.send(line);
    });
});

http.listen(PORT, () => {
    console.log(`Server: http://localhost:${PORT}`);
    console.log('Waiting for XR50 pose data on stdin...');
});

