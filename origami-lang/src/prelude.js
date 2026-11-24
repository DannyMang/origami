(() => {
  const T = t => typeof t === 'number' ? t : String(t).split(':').reduce((a, x) => a * 60 + +x, 0)
  const S = { fps: 30, size: null, preview: 'auto', proxy: true }
  let L = {}, code = '', at = 0, N = 0, P = [], D = 0, W = [1, 1], note
  const M = new Map(), reads = ['x', 'y', 'scale', 'rotate', 'opacity'], even = n => Math.round(n / 2) * 2
  const digits = /^\d+$/, share = /^[\d.]+%$/, written = 'keys are written keys([0, 0], ["50%", 1], [-0.5, 0])', named = /(?:const|let|var)\s+(\w+)\s*=/, made = 'video sound image text captions caption color', bare = 'keys rect line path add mul cat stack settings delay bezier'
  const DAY = 86400, run = Symbol(), calm = Symbol()
  const lined = (say, n = at) => n ? `line ${n}: ${say}` : say
  const here = () => +(new Error().stack?.match(/script:(\d+):/)?.[1] ?? 0)
  const must = (ok, say) => { if (!ok) throw new Error(lined(say)) }
  const fit = (k, v, lo = -Infinity, hi = Infinity, n, most = 'most it takes') => {
    const x = typeof v === 'number' ? Math.min(Math.max(v, lo), hi) : v
    if (x !== v) note ??= lined(`${k} ${v} is ${x > v ? 'raised' : 'lowered'} to ${x}, the ${x > v ? 'least it takes' : most}`, n)
    return x
  }
  const held = (k, v, lo, hi, most) => typeof v === 'function' ? Object.assign(v, { held: x => fit(k, x, lo, hi, v.line, most) }) : fit(k, v, lo, hi, at, most)
  const fin = (k, v, lo, hi) => {
    if (!Number.isFinite(v)) throw new Error(lined(`${k} takes a number`))
    return fit(k, v, lo, hi)
  }
  const opts = (k, o, names) => names.forEach(n => fin(`${k} ${n}`, o[n] ?? 0))
  const dotted = (k, p) => must(__dots(p), `${k} takes points of two numbers, like [[0, 0], [0.5, 0.6], [1, 1]]`)
  const vets = {
    look: (p, k = 100) => fin('look', k),
    curve: (n, p) => {
      must(typeof n === 'string', 'curve takes "luma", "red", "green" or "blue" first, like curve("luma", [[0, 0], [1, 1]])')
      dotted('curve', p)
    },
    wheel: (n, o) => {
      must(typeof n === 'string' && o, 'wheel takes "shadows", "midtones" or "highlights" first, like wheel("midtones", { hue: 40, amount: 0.4 })')
      opts('wheel', o, ['hue', 'amount', 'luma'])
    },
    vignette: o => opts('vignette', o, ['amount', 'midpoint', 'roundness', 'feather']),
    hslSecondary: ({ key, correction = {} }) => {
      opts('hslSecondary', key, ['range', 'soft'])
      opts('hslSecondary', correction, ['exposure', 'temperature', 'tint', 'contrast', 'saturation'])
    },
  }
  const hues = ['hueVsSat', 'hueVsHue', 'hueVsLuma', 'lumaVsSat', 'satVsSat'], tints = ['shadowTint', 'highlightTint'], dials = ['temperature', 'tint', 'saturation', 'exposure', 'contrast', 'highlights', 'shadows', 'whites', 'blacks', 'fadedFilm', 'vibrance', 'tintBalance']
  for (const k of hues) vets[k] = p => dotted(k, p)
  for (const k of tints) vets[k] = o => opts(k, o, ['hue', 'amount'])
  const paint = c => {
    must(/^#?([\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i.test(c), `${JSON.stringify(c)} is not a colour: write hex, like "#ffffff" or "#ffffff80", and click it for the colour wheel`)
    return c
  }
  const span = (k, t, some) => {
    const s = T(t)
    must(Math.abs(s) <= DAY && !(some && s <= 0), `${k} takes a time${some ? ' above 0' : ''}, in seconds or "m:ss", up to 24 hours`)
    return s
  }
  const whole = n => typeof n === 'string' && (String(n >>> 0) === n || digits.test(n))
  const line = g => new Proxy(g(0), { get: (t, n) => whole(n) ? g(+n) : t[n] })
  const anim = (k, v) => {
    if (v?.of) {
      const { of, k: p } = v, name = `${of.label ?? 'a group'}.${p === 'x' || p === 'y' ? 'pos.' : ''}${p}`
      if (of.kids || !reads.includes(p) || k.startsWith('each')) throw new Error(lined(`${k} cannot follow ${name} (pos.x, pos.y, scale, rotate and opacity of one clip can be followed, outside each)`))
      let f
      const led = T => (f ??= read(of, p))(T)
      v = Object.assign((t, d, T) => led(T), { [run]: (n, d, from) => Array.from({ length: n }, (_, i) => led(from + i / S.fps)) })
    }
    if (typeof v !== 'function' && !(typeof v === 'number' && v - v === 0)) throw new Error(lined(`${k} takes a number or a function of time`))
    return typeof v === 'number' ? v : Object.assign((t, d, T) => v(t, d, T), { line: at || here(), [run]: v[run], [calm]: v[calm] })
  }
  const num = (v, t, d, T) => typeof v === 'number' ? v : v(t, d, T)
  const times = (k, a, x) => {
    const b = anim(k, x)
    if (typeof a === 'number' && typeof b === 'number') return a * b - a * b === 0 ? a * b : anim(k, NaN)
    const all = typeof a === 'number' && b[run] && ((n, d, from) => b[run](n, d, from).map(x => a * x))
    return anim(k, Object.assign((t, d, T) => num(a, t, d, T) * num(b, t, d, T), { [run]: all, [calm]: all && b[calm] }))
  }
  const split = { __proto__: null, char: /\S/gu, word: /\S+/gu, line: /.+/gu }
  const fx = (d, f) => ({ ...d, fx: [...d.fx ?? [], f] })
  const ops = {
    __proto__: null,
    trim: (d, a, b) => {
      must(!d.kids, 'trim works on one clip, not a stack or cat: trim the clips inside')
      const top = d.total ? Math.ceil(d.total * 1e3) / 1e3 : DAY, i = Math.max(0, Math.min(span('trim', a), top - 0.04))
      return { ...d, in: i, out: Math.min(Math.max(b == null ? d.out : span('trim', b), i + 0.04), top) }
    },
    for: (d, s) => ({ ...d, dur: span('for', s, true) }),
    at: (d, t) => ({ ...d, at: span('at', t) }),
    shift: (d, s) => ({ ...d, shift: d.shift + span('shift', s) }),
    speed: (d, x) => {
      must(!d.kids, 'speed goes before merge, like stack(a, b).speed(2).merge(), and speeds up each clip inside')
      must(x > 0, 'speed takes a number above 0')
      return { ...d, speed: d.speed * x }
    },
    volume: (d, x) => ({ ...d, gain: d.gain * x }),
    mute: d => ({ ...d, gain: 0 }),
    fade: (d, a, b = a) => {
      must([a, b].every(x => Number.isFinite(x) && x >= 0), 'fade takes seconds, 0 or more')
      return { ...d, fade: [a, b] }
    },
    scale: (d, x) => ({ ...d, scale: times('scale', d.scale, x) }),
    pos: (d, x, y) => ({ ...d, pos: (x?.k === 'pos' && y === undefined ? [x.x, x.y] : [x, y]).map(v => v == null ? null : anim('pos', v)) }),
    opacity: (d, x) => ({ ...d, opacity: times('opacity', d.opacity, x) }),
    rotate: (d, x) => ({ ...d, rotate: anim('rotate', x) }),
    shown: (d, x) => ({ ...d, shown: anim('shown', x) }),
    each: (d, by, f) => {
      if (!split[by] || typeof f !== 'function') throw new Error(lined(`each is written each("char", i => ({ opacity: 1 })) with "char", "word" or "line"`))
      const m = [...d.src.matchAll(split[by])], n = s => [...s].length
      return {
        ...d,
        parts: m.map((w, i) => {
          const o = f(i, m.length), from = n(d.src.slice(0, w.index)), v = (k, z) => anim(`each ${k}`, o[k] ?? z)
          return { from, to: from + n(w[0]), opacity: v('opacity', 1), x: v('x', 0), y: v('y', 0), scale: v('scale', 1), rotate: v('rotate', 0) }
        }),
      }
    },
    blend: (d, m) => {
      must(m === 'multiply' || m === 'normal', 'blend is "multiply" or "normal"')
      return { ...d, blend: m === 'normal' ? '' : m }
    },
    fps: (d, n) => {
      must(n > 0 && n <= 240, 'fps takes a number above 0, up to 240')
      return { ...d, step: n }
    },
    matte: (d, m, how = 'alpha') => {
      must(m?.$, 'matte takes a clip, like title.matte(shape)')
      must(['alpha', 'alphaInverted', 'luma', 'lumaInverted'].includes(how), 'matte is "alpha", "alphaInverted", "luma" or "lumaInverted"')
      return { ...d, matte: [m.$, how] }
    },
    lens: (d, fov) => fx(d, { lens: held('lens', anim('lens', fov), -170, 170) }),
    blur: (d, r) => fx(d, { blur: held('blur', anim('blur', r), -Infinity, 16384, 'widest a picture can be') }),
    levels: (d, black, white, channel = 'rgb') => {
      must(['rgb', 'alpha'].includes(channel), 'levels is written levels(0.2, 0.8) or levels(0.2, 0.8, "alpha")')
      const b = fin('levels black', black, 0, 0.99)
      return fx(d, { levels: [b, fin('levels white', white, Math.round(b * 100 + 1) / 100, 1), channel === 'alpha'] })
    },
    noise: (d, amount, seed = 0) => fx(d, { noise: [fin('noise', amount, 0, 1), anim('noise', seed)] }),
    fractal: (d, o = {}) => fx(d, { fractal: { scale: fin('fractal scale', o.scale ?? 100, 1), seed: anim('fractal seed', o.seed ?? 0) } }),
    roughen: (d, o = {}) => fx(d, {
      roughen: {
        border: fin('roughen border', o.border ?? 8, 0), scale: fin('roughen scale', o.scale ?? 100, 1), sharp: fin('roughen sharpness', o.sharpness ?? 1, 0),
        seed: anim('roughen seed', o.seed ?? 0),
      },
    }),
    displace: (d, o = {}) => fx(d, {
      displace: { amount: anim('displace amount', o.amount ?? 50), size: fin('displace size', o.size ?? 100, 1), seed: anim('displace seed', o.seed ?? 0) },
    }),
    merge: d => {
      must(d.kids, 'merge is for a stack or cat')
      return { ...d, kind: 'group', src: '', video: true, audio: false, label: 'group' }
    },
    loop: (d, n) => {
      must(Number.isInteger(n) && n > 0 && n <= 1000, 'loop takes a whole number from 1 to 1000')
      return { ...d, loop: n }
    },
  }
  const grade = (d, g) => ({ ...d, grade: { ...d.grade, ...g } })
  const sorted = p => [...p].sort((a, b) => a[0] - b[0])
  const part = (k, names, d, n, v) => {
    if (!names.includes(n)) throw new Error(lined(`${k}("${n}") must be one of ${names.join(', ')}`))
    return grade(d, { [k]: { ...d.grade?.[k], [n]: v } })
  }
  const tone = {
    sharpen: d => ({ ...d }),
    look: (d, p, k = 100) => grade(d, { look: [__lut(p, 'look', at), k] }),
    inputLut: (d, p) => grade(d, { inputLut: __lut(p, 'inputLut', at) }),
    curve: (d, n, p) => part('curve', ['luma', 'red', 'green', 'blue'], d, n, sorted(p)),
    wheel: (d, n, o) => part('wheel', ['shadows', 'midtones', 'highlights'], d, n, [o.hue ?? 0, o.amount ?? 0, o.luma ?? 0]),
    hslSecondary: (d, { key, correction: c = {} }) => grade(d, {
      hslSecondary: {
        color: paint(key.color), range: key.range ?? 20, soft: key.soft ?? 10,
        exposure: c.exposure ?? 0, temperature: c.temperature ?? 0, tint: c.tint ?? 0, contrast: c.contrast ?? 0, saturation: c.saturation ?? 100,
      },
    }),
    duotone: (d, a, b) => grade(d, { duotone: [paint(a), paint(b)] }),
    vignette: (d, o) => grade(d, { vignette: { amount: o.amount ?? 0, midpoint: o.midpoint ?? 50, roundness: o.roundness ?? 0, feather: o.feather ?? 50 } }),
  }
  for (const k of dials) tone[k] = (d, x) => grade(d, { [k]: x })
  for (const k of hues) tone[k] = (d, p) => grade(d, { [k]: sorted(p) })
  for (const k of tints) tone[k] = (d, o) => grade(d, { [k]: [o.hue ?? 0, o.amount ?? 0] })
  Object.assign(ops, tone)
  const deep = ['speed', 'volume', 'mute', 'fade', 'scale', 'pos', 'opacity', 'rotate', 'shown', 'each', 'blend', 'fps', 'matte', 'lens', 'blur', 'levels', 'noise', 'fractal', 'roughen', 'displace', ...Object.keys(tone)]
  const loud = ['volume', 'mute']
  const nums = new Set(['speed', 'volume', 'loop', 'fps', 'sharpen', ...dials])
  const called = Object.keys(ops).join(' ')
  const work = (d, k, n, a) => {
    if (nums.has(k) && !Number.isFinite(a[0])) throw new Error(lined(`${k} takes a number`))
    if (n && vets[k]) vets[k](a[0], a[1])
    if (!d.kids && d.kind !== 'text' && (k === 'shown' || k === 'each')) throw new Error(lined(`${k} is for text and caption`))
    const o = d.kids && (d.kind ? loud : deep).includes(k) ? { ...d, kids: d.kids.map(c => work(c.$, k, 0, a)) } : ops[k](d, ...a)
    o.line = n || d.line
    return clip(o)
  }
  const act = (d, k, n) => {
    const f = (...a) => {
      at = n
      return work(d, k, n, a)
    }
    f.of = d
    f.k = k
    return k === 'pos' ? Object.assign(f, { x: { of: d, k: 'x' }, y: { of: d, k: 'y' } }) : f
  }
  const opaque = () => { throw new Error('a clip cannot be used as text or a number') }
  const chained = { get: (d, k) => k === '$' ? d : k in ops ? line(n => act(d, k, n)) : k === Symbol.toPrimitive ? opaque : Object.hasOwn(d, k) ? d[k] : undefined }
  const clip = d => new Proxy(d, chained)
  const base = { in: 0, out: null, speed: 1, gain: 1, fade: [0, 0], scale: 1, opacity: 1, rotate: 0, shown: 1, shift: 0, loop: 1 }
  const leaf = f => line(n => (...a) => { at = n; const d = f(...a); return clip({ ...base, ...d, line: n, home: n, label: L[n] ?? d.label, id: ++N }) })
  const stem = p => p.split('/').pop().replace(/\.\w+$/, '')
  const file = (kind, pick) => leaf(p => {
    const m = JSON.parse(__probe(p, kind, at))
    must(kind === 'sound' ? m.audio : kind === 'image' ? m.video && m.dur <= 0.04 : m.video || m.audio, `${kind}("${p}") cannot be read as ${kind === 'image' ? 'a still image' : kind}`)
    return { kind, src: m.path, w: m.w, h: m.h, label: stem(p), ...pick(m) }
  })
  const words = (size, pos) => leaf((s, o = {}) => {
    if (typeof o !== 'object') throw new Error(lined(`options are written { size: 72, color: "#ffffff" }`))
    if (!['left', 'center', 'right'].includes(o.align ?? 'center')) throw new Error(lined(`align is "left", "center" or "right"`))
    must(Number.isFinite(o.size ?? size) && (o.size ?? size) >= 0, 'size takes a number, 0 or more')
    return {
      kind: 'text', src: String(s), video: true, audio: false, size: o.size ?? size, color: paint(o.color ?? '#ffffff'), font: o.font == null ? '' : __font(o.font, 'font', at), pos, label: String(s),
      background: o.background == null ? o.background : paint(o.background), align: o.align,
    }
  })
  const shape = (kind, n, how, make) => leaf((...a) => {
    const o = a[n] ?? {}
    if (!(a.length >= n && a.slice(0, n).every(Number.isFinite)) || typeof o !== 'object') throw new Error(lined(`${kind} is written ${how}`))
    return { kind, src: '', video: true, audio: false, color: paint(o.color ?? '#ffffff'), label: kind, ...make(a, o) }
  })
  const group = (cat, k) => line(n => (...kids) => {
    at = n
    kids.flat().forEach((c, i) => must(c?.$, `${k} takes clips, but item ${i + 1} is not a clip`))
    return clip({ ...base, cat, kids: kids.flat() })
  })
  const pile = group(false, 'stack')
  const out3 = x => 1 - (1 - x) ** 3, tame = f => Object.assign(f, { [calm]: true })
  const eases = {
    __proto__: null,
    linear: x => x,
    step: () => 0,
    slowIn: x => x ** 3,
    slowOut: out3,
    slowInSlowOut: x => x < 0.5 ? 4 * x ** 3 : out3(2 * x - 1) / 2 + 0.5,
    springOut: x => 1 - Math.exp(-6 * x) * Math.cos(4.5 * Math.PI * x),
    overshootOut: x => 1 + 2.70158 * (x - 1) ** 3 + 1.70158 * (x - 1) ** 2,
    bounceOut: x => {
      const [c, b] = x < 1 / 2.75 ? [0, 0] : x < 2 / 2.75 ? [1.5, 0.75] : x < 2.5 / 2.75 ? [2.25, 0.9375] : [2.625, 0.984375]
      return 7.5625 * (x - c / 2.75) ** 2 + b
    },
  }
  Object.values(eases).forEach(tame)
  const wire = (k, f) => line(n => (v, x) => {
    at = n
    if (!Number.isFinite(x)) throw new Error(lined(`${k} is written ${k}(title.pos.y, 0.5)`))
    const g = anim(k, v)
    const all = g[run] && ((n, d, from) => g[run](n, d, from).map(y => f(y, x)))
    return typeof g === 'number' ? f(g, x) : Object.assign((t, d, T) => f(g(t, d, T), x), { [run]: all, [calm]: g[calm] })
  })
  Object.assign(globalThis, {
    add: wire('add', (a, b) => a + b),
    mul: wire('mul', (a, b) => a * b),
    keys: line(n => (...k) => {
      at = n
      const e = []
      let odd, D, T
      for (let i = __plain(k) ? k.length : 0; i < k.length; i++) {
        const p = k[i]
        if (!Array.isArray(p) || !Number.isFinite(p[0]) && !share.test(p[0]) || !Number.isFinite(p[1])) throw new Error(lined(`${written}`))
        const f = p[2] === undefined ? 'linear' : p[2]
        if (typeof f !== 'function' && !eases[f]) odd ??= [f]
        e.push(eases[f] ?? f)
      }
      must(k.length, written)
      if (odd) throw new Error(lined(`"${odd[0]}" is not an ease (${Object.keys(eases).join(', ')}, bezier)`))
      const linear = e.every((f, i) => !i || f === eases.linear)
      const lay = d => {
        if (d !== D) [D, T] = [d, k.map(([s]) => typeof s === 'string' ? parseFloat(s) / 100 * d : s < 0 ? d + s : s)]
      }
      const mix = (t, i) => i < 1 ? k.at(i)[1] : k[i - 1][1] + (k[i][1] - k[i - 1][1]) * (e[i] ?? eases.linear)((t - T[i - 1]) / (T[i] - T[i - 1]))
      const one = (t, d) => {
        lay(d)
        return mix(t, T.findIndex(s => t < s))
      }
      const slow = (n, d) => {
        lay(d)
        const s = []
        for (let f = 0, i = 0; f < n; f++) {
          while (i < T.length && !(f / S.fps < T[i])) i++
          s.push(mix(f / S.fps, i < T.length ? i : -1))
        }
        return s
      }
      const all = (n, d) => linear && __ramp(k, d, n, S.fps) || slow(n, d)
      return Object.assign(one, { [run]: all, [calm]: e.every(f => f[calm]) })
    }),
    bezier: line(n => (x1, y1, x2, y2) => {
      at = n
      must([x1, y1, x2, y2].every(Number.isFinite), 'bezier is written bezier(0.3, 0, 0.6, 1)')
      return tame(x => {
        const B = (p, q, s) => 3 * p * s * (1 - s) ** 2 + 3 * q * s * s * (1 - s) + s ** 3
        let lo = 0, hi = 1
        for (let i = 0; i < 24; i++) B(x1, x2, (lo + hi) / 2) < x ? lo = (lo + hi) / 2 : hi = (lo + hi) / 2
        return B(y1, y2, (lo + hi) / 2)
      })
    }),
    delay: line(n => (v, s) => {
      at = n
      fin('delay', s)
      return (t, d, T) => v(t - s, d, T - s)
    }),
    settings: line(n => (o = {}) => {
      at = n
      must(o.fps === undefined || Number.isInteger(o.fps) && o.fps > 0 && o.fps <= 240, 'fps takes a whole number from 1 to 240')
      must(o.size == null || Array.isArray(o.size) && o.size.length === 2 && o.size.every(v => v >= 1 && v <= 16384), 'size is written [1920, 1080], two numbers from 1 to 16384')
      if (o.size?.some(v => v % 2)) note ??= lined(`size [${o.size.join(', ')}] is rounded to [${o.size.map(even).join(', ')}], video files need even numbers`)
      must([undefined, 'auto', 'full', 'half', 'quarter'].includes(o.preview), 'preview is "auto", "full", "half" or "quarter"')
      must([undefined, true, false].includes(o.proxy), 'proxy is true or false')
      for (const k in S) if (o[k] !== undefined) S[k] = o[k]
    }),
    video: file('video', m => ({ video: m.video, audio: m.audio, out: m.dur, total: m.dur })),
    sound: file('sound', m => ({ video: false, audio: m.audio, out: m.dur, total: m.dur })),
    image: file('image', () => ({ video: true, audio: false })),
    text: words(72),
    caption: words(44, [null, 0.84]),
    captions: line(n => p => pile(JSON.parse(__read(p, 'captions', n)).map(([row, b]) => {
      const [cue, a, z, s] = b.match(/^(\S+) --> (\S+)\n([\s\S]+)/m) ?? [], t = x => T(x.replace(',', '.'))
      at = n
      if (a && !(t(z) > t(a))) must(false, `captions("${p}") line ${row + b.slice(0, b.indexOf(cue)).split('\n').length - 1}: ${a} --> ${z} is not a time range like 00:00:01,000 --> 00:00:04,000`)
      return a && caption[n](s.trim().replace(/\n/g, ' ')).at[n](t(a)).for[n](t(z) - t(a))
    }).filter(Boolean))),
    color: leaf(c => {
      must(typeof c === 'string', 'color is written color("#203040")')
      return { kind: 'color', src: paint(c), video: true, audio: false, label: c }
    }),
    rect: shape('rect', 2, 'rect(400, 120, { color: "#0008", radius: 12 })', ([w, h], o) => ({ w: Math.max(1, Math.round(w)), h: Math.max(1, Math.round(h)), radius: fin('radius', o.radius ?? 0) })),
    line: shape('line', 4, 'line(0.1, 0.5, 0.9, 0.5, { color: "#ffffff", width: 4 })', (a, o) => ({ ends: a.slice(0, 4), size: fin('width', o.width ?? 4) })),
    path: leaf((p, o = {}) => {
      must(Array.isArray(p) && p.length > 2 && p.every(q => Array.isArray(q) && q.length === 2) && typeof o === 'object', 'path is written path([[0.2, 0.2], [0.8, 0.3], [0.5, 0.8]], { color: "#ffffff", curve: true })')
      return { kind: 'path', src: '', video: true, audio: false, color: paint(o.color ?? '#ffffff'), label: 'path', points: p.map(q => q.map(v => anim('path', v))), curve: !!o.curve }
    }),
    cat: group(true, 'cat'),
    stack: pile,
  })
  const vid = d => d.kids ? d.kids.some(c => vid(c.$)) : d.video
  const own = d => d.dur ?? (d.out == null ? null : (d.out - d.in) / d.speed)
  const once = d => {
    if (d.dur != null) return d.dur
    const k = d.kids.map(c => c.$), end = c => (c.at ?? 0) + c.shift + (len(c) ?? 0)
    const m = d.cat
      ? Math.max(k.filter(vid).reduce((a, c) => a + (len(c) ?? 5), 0), ...k.filter(c => !vid(c)).map(end))
      : Math.max(0, ...k.map(end))
    return m > 0 ? m : null
  }
  const len = d => {
    if (!d.kids) return own(d) == null ? null : own(d) * d.loop
    const m = once(d)
    return m && m * d.loop
  }
  const upto = (x, stop) => x > stop + 1e-9 ? stop : x
  const cut = (d, t, n, out) => {
    if (!d.matte) return
    const [m, how] = d.matte, id = ++N, k = out.length
    place(m, t + (m.at ?? 0) + m.shift, n, out, id)
    out.slice(k).forEach(c => c.gain = 0)
    return [id, how]
  }
  const place = (d, t, fill, out, into = 0, stop = Infinity) => {
    if (!d.kids) {
      const n = own(d) ?? fill
      for (let i = 0; i < d.loop && t + i * n < stop - 1e-9; i++) {
        const s = t + i * n
        out.push({ ...d, start: s, end: upto(t + (i + 1) * n, stop), into, matte: cut(d, s, upto(n, stop - s), out) })
      }
      return
    }
    const D = once(d) ?? fill
    for (let i = 0; i < d.loop && t + i * D < stop - 1e-9; i++) {
      const s = t + i * D, end = d.dur == null ? stop : upto(s + D, stop)
      if (d.kind) out.push({ ...d, start: s, end: upto(s + D, stop), into, matte: cut(d, s, upto(D, stop - s), out), id: ++N })
      const inner = d.kind ? N : into
      let p = 0
      for (const c of d.kids.map(c => c.$)) {
        const seq = d.cat && vid(c), o = (c.at ?? (seq ? p : 0)) + c.shift, n = len(c) ?? (d.cat ? 5 : D - o)
        place(c, s + o, n, out, inner, end)
        if (seq) p += n
      }
    }
  }
  globalThis.__prep = src => {
    L = {}
    __assigns(src).forEach(([n, s]) => {
      const m = s.match(named)
      if (m) L[n] = m[1]
    })
    return code = __tag(src, made, bare, called)
  }
  const tail = () => __src.split('\n').findLastIndex(s => /^\s*[^\s/]/.test(s)) + 1 || 1
  const loose = () => {
    let q = ''
    return __src.split('\n').findIndex(s => {
      s = s.replace(/\\./g, '')
      for (let i = 0; i < s.length && (q || !s.startsWith('//', i)); i++) q = q ? s[i] === q ? '' : q : '"\'`'.includes(s[i]) ? s[i] : ''
      return q && q !== '`'
    }) + 1
  }
  globalThis.__late = m => {
    const [top, ...stack] = m.split('\n'), [, n, col] = m.match(/\n.*script:(\d+):(\d+)/) ?? m.match(/script:(\d+):(\d+)/) ?? []
    const head = top.replace(/ \((unknown at |script:|native\)).*/, '').replace(/^Error: /, '')
      .replace(/^RuntimeLimitError.*/, 'script took too long: a loop, or a function that calls itself, never ends')
    const lines = f => __src.split('\n').flatMap((s, i) => f(s, i + 1) ? [i + 1] : []), only = l => l.length === 1 ? l[0] : 0
    const name = head.match(/^ReferenceError: (.+) is not defined$/)?.[1]
    if (name) {
      const uses = lines((s, i) => s.split(/[^\w$]+/).filter(w => w === name).length > (L[i] === name))
      const row = only(uses) || (uses.includes(+n) ? +n : 0), below = Object.keys(L).find(l => L[l] === name && +l > row)
      return !row ? head : below ? `line ${row}: ${name} is defined below, on line ${below}; a clip can only follow one defined above it` : `line ${row}: ${head}`
    }
    const [, slip, spot] = head.match(/^(SyntaxError: .*) at line (\d+), col \d+$/) ?? []
    const spins = /iteration loops/.test(top) && stack[0]?.includes('(script:')
    const row = /\bline \d/.test(head) && !slip ? 0 : /unterminated string/.test(head) ? loose() : /abrupt end/.test(head) ? tail() : spot ?? (spins ? only(lines(s => /(?<![.\w$])(for|while|do)(?![\w$])/.test(s))) : n)
    const odd = head === 'TypeError: not a callable function' && [...code.split('\n')[n - 1] ?? ''].slice(0, col - 1).join('').match(/\.\s*([\w$]+)\s*$/)?.[1]
    const what = odd ? `${odd} is not a clip method: check the spelling, or the script may need a newer Origami` : slip ?? head
    return row ? `line ${row}: ${what}` : what
  }
  const frames = d => Math.max(1, Math.ceil(d * S.fps - 1e-6))
  const sample = (c, v) => {
    if (typeof v !== 'function') return v
    const d = c.end - c.start, n = frames(d), s = v[run] ? v[run](n, d, c.start) : []
    for (let i = s.length; i < n; i++) s.push(v(i / S.fps, d, c.start + i / S.fps))
    const kept = __keep(s)
    if (kept === undefined) throw new Error(lined('a function of time must return a number', v.line))
    return kept === false ? v.held?.(s[0]) ?? s[0] : [v.line, kept]
  }
  const drawn = (c, v) => typeof v === 'function' ? sample(c, v) : Array.isArray(v) ? v.map(x => drawn(c, x)) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, drawn(c, x)])) : v
  const ir = (c, z) => ({
    line: c.line, kind: c.kind, src: c.src, w: c.kind === 'group' ? W[0] : c.w ?? 0, h: c.kind === 'group' ? W[1] : c.h ?? 0, id: c.id, into: c.into, start: c.start, end: c.end, src_in: c.in, speed: c.speed, z,
    scale: sample(c, c.scale), pos: (c.pos ?? [null, null]).map(v => v == null ? null : sample(c, v)), gain: c.gain, fade: c.fade.map(f => Math.min(f, c.end - c.start)), label: c.label, home: c.home, total: c.total ?? 0,
    video: c.video, audio: c.audio, size: c.size ?? 0, color: c.color ?? '', font: c.font ?? '', grade: c.grade,
    radius: c.radius ?? 0, ends: c.ends ?? [0, 0, 0, 0], background: c.background ?? '', align: c.align ?? '',
    opacity: sample(c, c.opacity), rotate: sample(c, c.rotate), shown: sample(c, c.shown),
    parts: c.parts?.map(p => Object.fromEntries(Object.entries(p).map(([k, v]) => [k, sample(c, v)]))),
    blend: c.blend ?? '', step: c.step ?? 0, matte: c.matte, points: c.points?.map(q => q.map(v => sample(c, v))), curve: c.curve,
    fx: c.fx?.map(f => drawn(c, f)),
  })
  const read = (d, k) => {
    const s = P.find(c => c.id === d.id) ?? { start: 0, end: own(d) ?? D }, n = s.end - s.start, seen = []
    const get = (v, j) => num(v, j / S.fps, n, s.start + j / S.fps)
    const i = ['x', 'y'].indexOf(k), v = i < 0 ? d[k] : d.pos?.[i]
    let all
    const look = j => {
      if (v?.[calm]) return (all ??= v[run](frames(n), n, s.start))[j]
      if (v != null) return get(v, j)
      if (!M.has(d)) {
        const top = Math.max(...Array.from({ length: frames(n) }, (_, j) => get(d.scale, j)))
        const still = JSON.stringify(ir({ ...d, start: 0, end: n, scale: top, pos: null, opacity: 1, rotate: 0, shown: 1, parts: null }, 0))
        M.set(d, [top, [0, 1].map(a => __measure(still, ...W, a))])
      }
      const [top, box] = M.get(d), mid = d.kind === 'line' ? (d.ends[i] + d.ends[i + 2]) / 2 : 0.5
      return mid - box[i] * get(d.scale, j) / top / 2 / W[i]
    }
    return T => {
      const j = Math.min(Math.max(Math.round((T - s.start) * S.fps), 0), frames(n) - 1)
      return seen[j] ??= look(j)
    }
  }
  globalThis.__out = r => {
    if (!r || !r.$) throw new Error(`line ${tail()}: script must end with a clip`)
    const root = r.$
    place(root, 0, len(root) ?? 5, P)
    P = P.filter(c => c.video || c.gain > 0)
    const late = P.find(c => !(c.start >= -DAY && c.end <= DAY))
    if (late) throw new Error(`line ${late.line}: ${late.label} would end more than 24 hours in`)
    const first = P.find(c => c.kind === 'video' && c.w)
    W = (S.size ?? (first ? [first.w, first.h] : [1920, 1080])).map(even)
    D = Math.max(0, ...P.map(c => c.end))
    const clips = P.map(ir)
    return JSON.stringify({ fps: S.fps, size: W, duration: D, preview: S.preview, proxy: S.proxy, note, clips })
  }
  for (const k of ['__prep', '__late', '__out', '__probe', '__read', '__lut', '__font', '__measure', '__assigns', '__tag', '__plain', '__dots', '__ramp', '__keep']) Object.defineProperty(globalThis, k, { writable: false, configurable: false })
})()
