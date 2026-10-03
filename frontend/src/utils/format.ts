// Formatadores únicos com escala automática (task 17.6 R-01)

function scaled(value: number, base: number, units: string[], decimals: (v: number) => number): string {
  let i = 0
  let v = value
  while (Math.abs(v) >= base && i < units.length - 1) {
    v /= base
    i++
  }
  return `${v.toFixed(i === 0 ? 0 : decimals(v))} ${units[i]}`
}

/** Volume: B · KB · MB · GB · TB · PB (base 1024) */
export function formatBytes(bytes: number): string {
  return scaled(bytes, 1024, ['B', 'KB', 'MB', 'GB', 'TB', 'PB'], () => 1)
}

/** Rate: bps · kbps · Mbps · Gbps · Tbps (base 1000) */
export function formatBps(bps: number): string {
  return scaled(bps, 1000, ['bps', 'kbps', 'Mbps', 'Gbps', 'Tbps'], (v) => (Math.abs(v) < 10 ? 2 : 1))
}

/** Rate for series already in Mbps (charts) */
export function formatMbps(mbps: number): string {
  return formatBps(mbps * 1e6)
}

/** Packet rate: pps · kpps · Mpps */
export function formatPps(pps: number): string {
  return scaled(pps, 1000, ['pps', 'kpps', 'Mpps', 'Gpps'], (v) => (Math.abs(v) < 10 ? 2 : 1))
}

/** Average rate (bps) of `bytes` spread over `seconds` */
export function bytesToBps(bytes: number, seconds: number): number {
  return seconds > 0 ? (bytes * 8) / seconds : 0
}

export function formatNumber(n: number): string {
  return n.toLocaleString('pt-BR')
}
