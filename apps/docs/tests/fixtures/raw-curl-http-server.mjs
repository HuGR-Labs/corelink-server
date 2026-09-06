import http from "node:http";
import { Buffer } from "node:buffer";
import fs from "node:fs";
import process from "node:process";

const portFile = process.env.CORELINK_HTTP_PORT_FILE;
const requestFile = process.env.CORELINK_HTTP_REQUEST_FILE;
const status = Number(process.env.CORELINK_HTTP_STATUS ?? "500");
if (!portFile || !requestFile) process.exit(64);

const server = http.createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", () => {
    fs.writeFileSync(
      requestFile,
      JSON.stringify({
        method: request.method,
        url: request.url,
        bytes: Buffer.concat(chunks).length,
      }),
    );
    response.statusCode = status;
    response.end(`fixture HTTP ${status}\n`);
  });
});

server.listen(0, "127.0.0.1", () => {
  const port = server.address().port;
  fs.writeFileSync(portFile, String(port));
  process.stdout.write(`${JSON.stringify({ ready: true, port })}\n`);
});

process.on("SIGTERM", () => {
  server.close();
  server.closeAllConnections?.();
  process.exit(0);
});
