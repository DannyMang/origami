export default {
  async fetch(request: Request, env: { DOWNLOADS: R2Bucket }) {
    const object = await env.DOWNLOADS.get(new URL(request.url).pathname.slice(1))
    if (!object) return new Response("Not found", { status: 404 })
    const headers = new Headers({ etag: object.httpEtag })
    object.writeHttpMetadata(headers)
    return new Response(object.body, { headers })
  },
}
