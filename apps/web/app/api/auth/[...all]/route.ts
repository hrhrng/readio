import type { NextRequest } from "next/server";

const cloudOrigin =
  process.env.READIO_CLOUD_URL ||
  process.env.NEXT_PUBLIC_READIO_CLOUD_URL ||
  "http://127.0.0.1:8787";

async function proxy(request: NextRequest): Promise<Response> {
  const upstream = new URL(request.nextUrl.pathname + request.nextUrl.search, cloudOrigin);
  const headers = new Headers(request.headers);
  headers.delete("host");
  headers.delete("content-length");

  try {
    return await fetch(upstream, {
      method: request.method,
      headers,
      body:
        request.method === "GET" || request.method === "HEAD"
          ? undefined
          : await request.arrayBuffer(),
      redirect: "manual",
    });
  } catch {
    return Response.json(
      {
        error: {
          code: "cloud_unavailable",
          message: "Cloud sign-in is unavailable. Local reading still works.",
        },
      },
      { status: 502 },
    );
  }
}

export const GET = proxy;
export const POST = proxy;
