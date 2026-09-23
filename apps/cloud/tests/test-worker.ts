export default {
  async fetch() {
    return new Response("test worker")
  },
} satisfies ExportedHandler<Env>
