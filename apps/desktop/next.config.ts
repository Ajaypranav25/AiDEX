import type { NextConfig } from "next";

const isProduction = process.env.NODE_ENV === "production";
// `next dev` binds 127.0.0.1 only; scripts and the hot-reload socket must come from the same address.
const devHost = process.env.TAURI_DEV_HOST ?? "127.0.0.1";

const config: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  assetPrefix: isProduction ? undefined : `http://${devHost}:3311`,
  transpilePackages: ["@aidex/client", "@aidex/ui"],
};

export default config;
