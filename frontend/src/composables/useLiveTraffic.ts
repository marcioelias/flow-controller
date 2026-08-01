// Estado vivo do tráfego (fatias por segundo vindas do /ws), em escopo de
// módulo: sobrevive à troca de views/abas do SPA — o WebSocket continua
// acumulando com o Dashboard desmontado, e um snapshot em sessionStorage
// restaura a janela após um reload (task 13.10).

export interface LiveBucket {
  v4i: number
  v4o: number
  v6i: number
  v6o: number
}

export const LIVE_WINDOW_SECS = 300
const SS_KEY = 'fv-live-window-v1'
const SAVE_EVERY_MS = 5_000

const buckets = new Map<number, LiveBucket>()
const listeners = new Set<(stats: any) => void>()
let ws: WebSocket | null = null
let started = false
let lastSave = 0

function nowSec(): number {
  return Math.floor(Date.now() / 1000)
}

export function pruneBuckets(map: Map<number, LiveBucket>) {
  const first = nowSec() - LIVE_WINDOW_SECS
  for (const sec of map.keys()) {
    if (sec < first) map.delete(sec)
  }
}

function bucketAdd(sec: number, v4i: number, v4o: number, v6i: number, v6o: number) {
  const b = buckets.get(sec)
  if (b) {
    b.v4i += v4i
    b.v4o += v4o
    b.v6i += v6i
    b.v6o += v6o
  } else {
    buckets.set(sec, { v4i, v4o, v6i, v6o })
  }
}

function restore() {
  try {
    const raw = sessionStorage.getItem(SS_KEY)
    if (!raw) return
    const rows: [number, number, number, number, number][] = JSON.parse(raw)
    const first = nowSec() - LIVE_WINDOW_SECS
    for (const [sec, v4i, v4o, v6i, v6o] of rows) {
      if (sec >= first) buckets.set(sec, { v4i, v4o, v6i, v6o })
    }
  } catch {
    sessionStorage.removeItem(SS_KEY)
  }
}

function save() {
  const now = Date.now()
  if (now - lastSave < SAVE_EVERY_MS) return
  lastSave = now
  pruneBuckets(buckets)
  try {
    const rows = [...buckets.entries()].map(
      ([sec, b]) => [sec, b.v4i, b.v4o, b.v6i, b.v6o] as const,
    )
    sessionStorage.setItem(SS_KEY, JSON.stringify(rows))
  } catch {
    // quota/privado — janela viva segue funcionando sem snapshot
  }
}

function connect() {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${proto}//${location.host}/ws`)

  ws.onmessage = (e) => {
    try {
      const stats = JSON.parse(e.data)
      if (!stats.timestamp_sec) return
      if (Array.isArray(stats.slices)) {
        for (const sl of stats.slices) {
          bucketAdd(sl.sec, sl.v4_in ?? 0, sl.v4_out ?? 0, sl.v6_in ?? 0, sl.v6_out ?? 0)
        }
      }
      save()
      for (const fn of listeners) fn(stats)
    } catch {}
  }
  ws.onclose = () => setTimeout(connect, 3000)
}

export function useLiveTraffic() {
  if (!started) {
    started = true
    restore()
    connect()
  }
  return {
    buckets,
    /** Recebe cada mensagem crua do WS (ex.: modo por dispositivo) */
    subscribe(fn: (stats: any) => void): () => void {
      listeners.add(fn)
      return () => listeners.delete(fn)
    },
  }
}
