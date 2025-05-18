import http from 'http';
import { URL } from 'url';

const PORT = 3000;
const MAX_DEPTH = 5;
const CHILDREN_PER_PAGE = 3;

function generateChildrenPaths(basePath, depth) {
  if (depth >= MAX_DEPTH) return [];
  const nextDepth = depth + 1;
  const children = [];
  for (let i = 0; i < CHILDREN_PER_PAGE; i++) {
    children.push(`${basePath}/${i}`);
  }
  return children;
}

function extractDepth(path) {
  const parts = path.split('/').filter(Boolean); // Remove empty strings
  return parts.length;
}

function handler(req, res) {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const path = url.pathname;
  const depth = extractDepth(path);

  if (depth > MAX_DEPTH) {
    res.writeHead(404);
    res.end('Not Found');
    return;
  }

  const children = generateChildrenPaths(path === '/' ? '' : path, depth);
  const body = `
    <html>
      <head><title>Depth ${depth}</title></head>
      <body>
        <h1>Depth ${depth}</h1>
        <ul>
          ${children.map(child => `<li><a href="http://localhost:3000${child}">${child}</a></li>`).join('\n')}
        </ul>
      </body>
    </html>
  `;

  res.writeHead(200, { 'Content-Type': 'text/html' });
  res.end(body);
}

const server = http.createServer(handler);
server.listen(PORT, () => {
  console.log(`Server running at http://localhost:${PORT}/`);
});
