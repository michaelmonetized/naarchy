import { join } from "node:path";

const root = join(import.meta.dir, "..");
const files = new Set([
  "/brand/naarchy-logo.png",
  "/gallery/01-home.png",
  "/gallery/02-files.png",
  "/gallery/03-clipboard.png",
  "/gallery/04-calendar.png",
]);
Bun.serve({
  hostname: "127.0.0.1",
  port: 3018,
  fetch(request) {
    const path = new URL(request.url).pathname;
    if (!files.has(path)) return new Response("Not found", { status: 404 });
    return new Response(Bun.file(join(root, path)), {
      headers: { "Access-Control-Allow-Origin": "*", "Content-Type": "image/png" },
    });
  },
});
console.log("Launch upload assets: http://127.0.0.1:3018");
