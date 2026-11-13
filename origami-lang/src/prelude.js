(() => {
  const T = t => typeof t === 'number' ? t : String(t).split(':').reduce((a, x) => a * 60 + +x, 0)
  const S = { fps: 30, size: null }
  let L = {}, at = 0
  const digits = /^\d+$/, named = /(?:const|let|var)\s+(\w+)\s*=/, made = /\b(video|sound|image|text|captions|caption|color)\(/g
  const line = g => new Proxy(g(0), { get: (t, n) => digits.test(String(n)) ? g(+n) : t[n] })
  const ops = {
    trim: (d, a, b) => ({ ...d, in: T(a), out: Math.max(b == null ? d.out : T(b), T(a) + 0.04) }),
    for: (d, s) => ({ ...d, dur: T(s) }),
    at: (d, t) => ({ ...d, at: T(t) }),
    shift: (d, s) => ({ ...d, shift: d.shift + T(s) }),
    speed: (d, x) => ({ ...d, speed: d.speed * x }),
    volume: (d, x) => ({ ...d, gain: d.gain * x }),
    mute: d => ({ ...d, gain: 0 }),
    fade: (d, a, b = a) => ({ ...d, fade: [a, b] }),
    scale: (d, x) => ({ ...d, scale: d.scale * x }),
    pos: (d, x, y) => ({ ...d, pos: [x, y] }),
    loop: (d, n) => ({ ...d, loop: n }),
  }
  const grade = (d, g) => ({ ...d, grade: { ...d.grade, ...g } })
  const sorted = p => [...p].sort((a, b) => a[0] - b[0])
  const part = (k, names, d, n, v) => {
    if (!names.includes(n)) throw new Error(`line ${at}: ${k}("${n}") must be one of ${names.join(', ')}`)
    return grade(d, { [k]: { ...d.grade?.[k], [n]: v } })
  }
  const tone = {
    sharpen: d => d,
    look: (d, p, k = 100) => grade(d, { look: [__lut(p, 'look', at), k] }),
    inputLut: (d, p) => grade(d, { inputLut: __lut(p, 'inputLut', at) }),
    curve: (d, n, p) => part('curve', ['luma', 'red', 'green', 'blue'], d, n, sorted(p)),
    wheel: (d, n, o) => part('wheel', ['shadows', 'midtones', 'highlights'], d, n, [o.hue ?? 0, o.amount ?? 0, o.luma ?? 0]),
    hslSecondary: (d, { key, correction: c = {} }) => grade(d, {
      hslSecondary: {
        color: key.color, range: key.range ?? 20, soft: key.soft ?? 10,
        temperature: c.temperature ?? 0, tint: c.tint ?? 0, contrast: c.contrast ?? 0, saturation: c.saturation ?? 100,
      },
    }),
    vignette: (d, o) => grade(d, { vignette: { amount: o.amount ?? 0, midpoint: o.midpoint ?? 50, roundness: o.roundness ?? 0, feather: o.feather ?? 50 } }),
  }
  for (const k of ['temperature', 'tint', 'saturation', 'exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'fadedFilm', 'vibrance', 'tintBalance'])
    tone[k] = (d, x) => grade(d, { [k]: x })
  for (const k of ['hueVsSat', 'hueVsHue', 'hueVsLuma', 'lumaVsSat', 'satVsSat']) tone[k] = (d, p) => grade(d, { [k]: sorted(p) })
  for (const k of ['shadowTint', 'highlightTint']) tone[k] = (d, o) => grade(d, { [k]: [o.hue ?? 0, o.amount ?? 0] })
  Object.assign(ops, tone)
  const deep = ['speed', 'volume', 'mute', 'fade', 'scale', 'pos', ...Object.keys(tone)]
  const nums = ['speed', 'volume', 'scale', 'loop', 'temperature', 'tint', 'saturation', 'exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'fadedFilm', 'vibrance', 'tintBalance']
  const called = new RegExp(`\\.(${Object.keys(ops).join('|')})\\(`, 'g')
  const clip = d => new Proxy(d, {
    get: (d, k) => k === '$' ? d : k in ops ? line(n => (...a) => {
      at = n || at
      if (nums.includes(k) && typeof a[0] !== 'number') throw new Error(`line ${at}: ${k} takes a number`)
      return clip({
        ...(d.kids && deep.includes(k) ? { ...d, kids: d.kids.map(c => c[k](...a)) } : ops[k](d, ...a)),
        line: n || d.line,
      })
    }) : d[k],
  })
  const base = { in: 0, out: null, speed: 1, gain: 1, fade: [0, 0], scale: 1, shift: 0, loop: 1 }
  const leaf = f => line(n => (...a) => { at = n; const d = f(...a); return clip({ ...base, ...d, line: n, label: L[n] ?? d.label }) })
  const stem = p => p.split('/').pop().replace(/\.\w+$/, '')
  const file = (kind, pick) => leaf(p => {
    const m = JSON.parse(__probe(p, kind, at))
    return { kind, src: m.path, w: m.w, h: m.h, label: stem(p), ...pick(m) }
  })
  const words = (size, pos) => leaf((s, o = {}) => {
    if (typeof o !== 'object') throw new Error(`line ${at}: options are written { size: 72, color: "red" }`)
    return { kind: 'text', src: String(s), video: true, audio: false, size: o.size ?? size, color: o.color ?? 'white', pos, label: String(s) }
  })
  const group = cat => (...kids) => clip({ ...base, cat, kids: kids.flat() })
  Object.assign(globalThis, {
    settings: o => { Object.assign(S, o) },
    video: file('video', m => ({ video: m.video, audio: m.audio, out: m.dur, total: m.dur })),
    sound: file('sound', m => ({ video: false, audio: m.audio, out: m.dur, total: m.dur })),
    image: file('image', () => ({ video: true, audio: false })),
    text: words(72),
    caption: words(44, [null, 0.84]),
    captions: line(n => p => group(false)(__read(p, 'captions', n).replace(/\r/g, '').split(/\n\n+/).map(b => {
      const [, a, z, s] = b.match(/(\S+) --> (\S+)\n([\s\S]+)/) ?? [], t = x => T(x.replace(',', '.'))
      return a && caption[n](s.trim().replace(/\n/g, ' ')).at[n](t(a)).for[n](t(z) - t(a))
    }).filter(Boolean))),
    color: leaf(c => ({ kind: 'color', src: c, video: true, audio: false, label: c })),
    cat: group(true),
    stack: group(false),
  })
  const vid = d => d.kids ? d.kids.some(c => vid(c.$)) : d.video
  const own = d => d.dur ?? (d.out == null ? null : (d.out - d.in) / d.speed)
  const len = d => {
    if (!d.kids) return own(d) == null ? null : own(d) * d.loop
    if (d.dur != null) return d.dur
    const k = d.kids.map(c => c.$), end = c => (c.at ?? 0) + c.shift + (len(c) ?? 0)
    const m = d.cat
      ? Math.max(k.filter(vid).reduce((a, c) => a + (len(c) ?? 5), 0), ...k.filter(c => !vid(c)).map(end))
      : Math.max(0, ...k.map(end))
    return m > 0 ? m : null
  }
  const place = (d, t, fill, out) => {
    if (!d.kids) {
      const n = own(d) ?? fill
      for (let i = 0; i < d.loop; i++) out.push({ ...d, start: t + i * n, end: t + (i + 1) * n })
      return
    }
    const D = len(d) ?? fill
    let p = 0
    for (const c of d.kids.map(c => c.$)) {
      const seq = d.cat && vid(c), o = (c.at ?? (seq ? p : 0)) + c.shift, n = len(c) ?? (d.cat ? 5 : D - o)
      place(c, t + o, n, out)
      if (seq) p += n
    }
  }
  globalThis.__prep = src => {
    L = {}
    return src.split('\n').map((s, i) => {
      const m = s.match(named)
      if (m) L[i + 1] = m[1]
      return s
        .replace(made, `$1[${i + 1}](`)
        .replace(called, `.$1[${i + 1}](`)
    }).join('\n')
  }
  globalThis.__out = r => {
    if (!r || !r.$) throw new Error('script must end with a clip')
    const out = [], root = r.$
    place(root, 0, len(root) ?? 5, out)
    const first = out.find(c => c.kind === 'video' && c.w)
    const even = n => Math.round(n / 2) * 2
    return JSON.stringify({
      fps: S.fps,
      size: (S.size ?? (first ? [first.w, first.h] : [1920, 1080])).map(even),
      duration: Math.max(0, ...out.map(c => c.end)),
      clips: out.map((c, z) => ({
        kind: c.kind, src: c.src, w: c.w ?? 0, h: c.h ?? 0, start: c.start, end: c.end, src_in: c.in, speed: c.speed, z,
        scale: c.scale, pos: c.pos ?? [null, null], gain: c.gain, fade: c.fade, label: c.label, line: c.line, total: c.total ?? 0,
        video: c.video, audio: c.audio, size: c.size ?? 0, color: c.color ?? '', grade: c.grade,
      })),
    })
  }
})()
