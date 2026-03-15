import path from "path";
import fs from "fs";
import { betterAuth } from "better-auth";
import Database from "better-sqlite3";

// Resolve to monorepo root's data/ directory.
// Try relative to CWD (works when CWD is apps/web/ or project root).
function resolveDbPath(): string {
  // If CWD already has data/, use it (running from project root)
  const fromCwd = path.resolve(process.cwd(), "data/auth.db");
  if (fs.existsSync(path.dirname(fromCwd))) return fromCwd;

  // Otherwise assume CWD is apps/web/, go up to root
  const fromWeb = path.resolve(process.cwd(), "../../data/auth.db");
  fs.mkdirSync(path.dirname(fromWeb), { recursive: true });
  return fromWeb;
}

const dbPath = resolveDbPath();

export const auth = betterAuth({
  database: new Database(dbPath),
  emailAndPassword: {
    enabled: true,
  },
  session: {
    cookieCache: {
      enabled: true,
      maxAge: 5 * 60, // 5 minutes
    },
  },
});
