import type { NextConfig } from "next";

const apiOrigin =
  process.env.READIO_API_ORIGIN ||
  process.env.NEXT_PUBLIC_API_URL ||
  "http://127.0.0.1:8000";

const nextConfig: NextConfig = {
  transpilePackages: ["@readio/sdk"],
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          {
            key: "Cross-Origin-Opener-Policy",
            value: "same-origin",
          },
          {
            key: "Cross-Origin-Embedder-Policy",
            value: "require-corp",
          },
        ],
      },
    ];
  },
  async rewrites() {
    return {
      beforeFiles: [],
      afterFiles: [
        {
          // Proxy all /api/* EXCEPT /api/auth/* to FastAPI
          source: "/api/:path((?!auth/).*)*",
          destination: `${apiOrigin}/api/:path*`,
        },
      ],
      fallback: [],
    };
  },
};

export default nextConfig;
