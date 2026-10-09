import { createHash } from "node:crypto";

const exports = [
	["naarchy-release-1080p.mp4", 1920, 1080, 1140],
	["naarchy-release-square.mp4", 1080, 1080, 1140],
	["naarchy-release-vertical.mp4", 1080, 1920, 1140],
	["made-in-omadesign.mp4", 1920, 1080, 720],
	["made-in-omadesign-vertical.mp4", 1080, 1920, 720],
] as const;
const files = [];
for (const [name, width, height, frames] of exports) {
	const path = `launch/artifacts/delivery/${name}`;
	const probe = Bun.spawn([
		"ffprobe",
		"-v",
		"error",
		"-select_streams",
		"v:0",
		"-show_entries",
		"stream=codec_name,width,height,duration,nb_frames,r_frame_rate",
		"-of",
		"json",
		path,
	]);
	const metadata = JSON.parse(await new Response(probe.stdout).text())
		.streams[0];
	if (
		(await probe.exited) !== 0 ||
		metadata.codec_name !== "h264" ||
		metadata.width !== width ||
		metadata.height !== height ||
		Number(metadata.nb_frames) !== frames ||
		metadata.r_frame_rate !== "30/1" ||
		Number(metadata.duration) !== frames / 30
	)
		throw new Error(`Incorrect video stream: ${name}`);
	const decode = Bun.spawn(
		["ffmpeg", "-v", "error", "-i", path, "-f", "null", "-"],
		{ stdout: "ignore", stderr: "pipe" },
	);
	const errors = await new Response(decode.stderr).text();
	if ((await decode.exited) !== 0 || errors.trim())
		throw new Error(`Decode failed: ${name}: ${errors}`);
	const bytes = await Bun.file(path).arrayBuffer();
	files.push({
		name,
		bytes: bytes.byteLength,
		sha256: createHash("sha256").update(new Uint8Array(bytes)).digest("hex"),
		...metadata,
		decode: "pass",
	});
}
await Bun.write(
	"launch/manifest.json",
	JSON.stringify(
		{
			verifiedAt: new Date().toISOString(),
			nativeRenderer: {
				product: "Omadesign",
				version: "0.6.3",
				commit: "c4813ef9a9628274778047e880a3aff6ea98d199",
				frames: 120,
				fps: 30,
				duration: 4,
				width: 1024,
				height: 1024,
			},
			filmRenderer: { product: "Remotion", version: "4.0.481" },
			files,
		},
		null,
		2,
	) + "\n",
);
console.log(`Verified ${files.length} exports.`);
