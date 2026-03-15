import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  async rewrites() {
    return {
      beforeFiles: [],
      afterFiles: [
        {
          // Proxy all /api/* EXCEPT /api/auth/* to FastAPI
          source: "/api/:path((?!auth/).*)*",
          destination: "http://127.0.0.1:8000/api/:path*",
        },
      ],
      fallback: [],
    };
  },
  webpack: (config) => {
    // pdf.js optional dependency
    config.resolve.alias.canvas = false;
    return config;
  },
};

export default nextConfig;
