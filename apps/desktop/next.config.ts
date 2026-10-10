import type { NextConfig } from "next";

const isProduction = process.env.NODE_ENV === "production";
const devHost = process.env.TAURI_DEV_HOST ?? "localhost";

const config: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  assetPrefix: isProduction ? undefined : `http://${devHost}:3311`,
  transpilePackages: ["@aidex/client", "@aidex/ui"],
};

export default config;
