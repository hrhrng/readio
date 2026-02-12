import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: "http://127.0.0.1:8000/api/:path*",
      },
    ];
  },
  webpack: (config) => {
    // pdf.js optional dependency
    config.resolve.alias.canvas = false;
    return config;
  },
};

export default nextConfig;
