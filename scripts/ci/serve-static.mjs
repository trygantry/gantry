import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';

const root = path.resolve(process.argv[2] || 'src');
const port = Number(process.env.PORT || 4173);
const host = process.env.HOST || '127.0.0.1';
const contentTypes = new Map([
  ['.css', 'text/css; charset=utf-8'],
  ['.html', 'text/html; charset=utf-8'],
  ['.ico', 'image/x-icon'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.png', 'image/png'],
  ['.svg', 'image/svg+xml'],
]);

const server = http.createServer((request, response) => {
  try {
    const requested = decodeURIComponent(new URL(request.url || '/', `http://${host}`).pathname);
    const relative = requested === '/' ? 'index.html' : requested.replace(/^\/+/, '');
    const candidate = path.resolve(root, relative);
    if (candidate !== root && !candidate.startsWith(`${root}${path.sep}`)) {
      response.writeHead(403).end('Forbidden');
      return;
    }
    const stat = fs.statSync(candidate);
    const file = stat.isDirectory() ? path.join(candidate, 'index.html') : candidate;
    response.writeHead(200, {
      'Cache-Control': 'no-store',
      'Content-Type': contentTypes.get(path.extname(file)) || 'application/octet-stream',
    });
    fs.createReadStream(file).pipe(response);
  } catch {
    response.writeHead(404).end('Not found');
  }
});

server.listen(port, host, () => {
  process.stdout.write(`Serving ${root} at http://${host}:${port}\n`);
});
