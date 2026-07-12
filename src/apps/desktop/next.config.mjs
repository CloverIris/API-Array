import { PHASE_PRODUCTION_BUILD } from "next/constants.js";

const isProduction = process.env.NODE_ENV === "production";
const internalHost = process.env.TAURI_DEV_HOST ?? "127.0.0.1";

/** @type {import("next").NextConfig} */
const createNextConfig = (phase) => ({
  output: "export",
  // Release bundling must not contend with a running development server for
  // Next's trace files. The exported `out/` directory remains the Tauri input.
  distDir: phase === PHASE_PRODUCTION_BUILD ? ".next-release" : ".next",
  images: { unoptimized: true },
  assetPrefix: isProduction ? undefined : `http://${internalHost}:3000`
});

export default createNextConfig;
