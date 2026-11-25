import { marked } from "marked"
import { createHash } from "node:crypto"
import { cpSync, mkdirSync, existsSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"

const root = import.meta.dir
const [pages, media, dist, videos] = ["pages", "media", "dist", "dist/video"].map(d => join(root, d))
const cli = join(root, "../origami-engine/target/release/origami")
const script = join(media, ".example.js")
const draft = join(media, ".example.mp4")

const hash = (...parts: (string | Uint8Array)[]) =>
  parts.reduce((h, p) => h.update(p), createHash("sha256")).digest("hex").slice(0, 16)
const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;")
const player = (src: string, label = "") =>
  `<video controls muted loop playsinline preload="metadata" src="${src}" aria-label="${escape(label)}"></video>`

const inputs = hash(
  readFileSync(cli),
  ...[...new Bun.Glob("**/*").scanSync(media)].sort().flatMap(f => [f, readFileSync(join(media, f))]),
)
const used = new Set<string>()
let rendered = 0

const render = (code: string) => {
  const name = hash(inputs, code) + ".mp4"
  used.add(name)
  if (existsSync(join(videos, name))) return name
  const start = performance.now()
  writeFileSync(script, code)
  const run = Bun.spawnSync([cli, "render", script, draft])
  rmSync(script)
  rmSync(join(media, "origami.lock"), { force: true })
  if (run.exitCode) {
    rmSync(draft, { force: true })
    console.error(`render failed:\n\n${code}\n\n${run.stderr.toString().trim()}`)
    process.exit(1)
  }
  renameSync(draft, join(videos, name))
  rendered++
  console.log(`rendered ${name} in ${((performance.now() - start) / 1000).toFixed(1)}s`)
  return name
}

type Page = { file: string; title: string; name: string; links: string[]; body: string }
let page: Page
marked.use({
  renderer: {
    heading({ tokens, depth, text }) {
      if (depth > 2) return false
      const html = this.parser.parseInline(tokens)
      const id = text.toLowerCase().replace(/\W+/g, "-")
      if (depth === 1) Object.assign(page, { title: html, name: text })
      else page.links.push(`<a href="#${id}">${html}</a>`)
      return `<h${depth} id="${id}">${html}</h${depth}>\n`
    },
    code({ text, lang }) {
      const pre = `<pre><code>${escape(text)}</code></pre>\n`
      return lang === "origami" ? pre + player("video/" + render(text)) + "\n" : pre
    },
    image({ href, text }) {
      if (!href.startsWith("media/") || href.split("/").includes("..")) throw new Error(`images must be under media/: ${href}`)
      mkdirSync(dirname(join(dist, href)), { recursive: true })
      cpSync(join(root, href), join(dist, href))
      return /\.(mp4|webm|mov)$/.test(href) ? player(href, text) : false
    },
  },
})

mkdirSync(videos, { recursive: true })
cpSync(join(root, "fonts"), join(dist, "fonts"), { recursive: true })
cpSync(join(root, "../latest.json"), join(dist, "latest.json"))
const site = readdirSync(pages).sort().map((f, i) => {
  page = { file: i ? f.replace(/^\d+-|\.md$/g, "") + ".html" : "index.html", title: "", name: "", links: [], body: "" }
  page.body = marked.parse(readFileSync(join(pages, f), "utf8")) as string
  return page
})
for (const f of readdirSync(videos)) if (!used.has(f)) rmSync(join(videos, f))
for (const f of readdirSync(dist)) if (f.endsWith(".html")) rmSync(join(dist, f))
const template = readFileSync(join(root, "template.html"), "utf8")
for (const p of site) {
  const nav = site.map(q =>
    q === p ? `<a href="${q.file}" aria-current="page">${q.title}</a><div>${q.links.join("")}</div>` : `<a href="${q.file}">${q.title}</a>`)
  writeFileSync(
    join(dist, p.file),
    template.replace("{{title}}", () => escape(p.name))
      .replace("{{nav}}", () => nav.join(""))
      .replace("{{toc}}", () => p.links.join("")).replace("{{main}}", () => p.body),
  )
}
console.log(`${site.length} pages, ${used.size} examples, ${rendered} rendered`)
