import { betterAuth } from "better-auth"
import { drizzleAdapter } from "better-auth/adapters/drizzle"
import { bearer, deviceAuthorization } from "better-auth/plugins"
import { drizzle } from "drizzle-orm/d1"

import { authSchema } from "./auth-schema"

export function createAuth(env: Env) {
  const database = drizzle(env.DB, { schema: authSchema })
  return betterAuth({
    database: drizzleAdapter(database, {
      provider: "sqlite",
      schema: authSchema,
    }),
    baseURL: env.BETTER_AUTH_URL,
    secret: env.BETTER_AUTH_SECRET,
    trustedOrigins: env.TRUSTED_ORIGINS.split(",").map((origin) => origin.trim()),
    emailAndPassword: {
      enabled: true,
    },
    session: {
      cookieCache: {
        enabled: true,
        maxAge: 5 * 60,
      },
    },
    plugins: [
      bearer({ requireSignature: true }),
      deviceAuthorization({
        verificationUri: `${env.WEB_URL}/device`,
        validateClient: async (clientId) =>
          clientId === "readio-tui" || clientId === "readio-apple",
      }),
    ],
  })
}

export type ReadioAuth = ReturnType<typeof createAuth>
