import { NextResponse } from "next/server";

/**
 * Reading is local-first. Authentication is a capability boundary for sync
 * and cloud TTS, never a gate in front of the app shell or local library.
 */
export function middleware() {
  return NextResponse.next();
}
