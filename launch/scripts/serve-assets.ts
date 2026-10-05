import { join } from "node:path";
const root=join(import.meta.dir,"..");
Bun.serve({port:3018,fetch(req){const p=decodeURIComponent(new URL(req.url).pathname);if(p.includes(".."))return new Response("Forbidden",{status:403});return new Response(Bun.file(join(root,p)),{headers:{"Access-Control-Allow-Origin":"*"}})}});
console.log("Launch assets: http://localhost:3018");
