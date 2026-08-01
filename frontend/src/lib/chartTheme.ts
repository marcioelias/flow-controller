// Tema único dos gráficos de tráfego (ao vivo + histórico).
// Pares validados via dataviz validate_palette na surface dark (zinc-900):
// in/out deutan ΔE 10.0; v4/v6 deutan ΔE 23+ — todos os checks PASS.
export const COLOR_IN = '#00a870' // entrada — verde vivo
export const COLOR_OUT = '#f05a24' // saída — laranja-avermelhado vivo
export const COLOR_UNKNOWN = '#a1a1aa' // sem direção — neutro deliberado
// Empilhado família × direção (task 13.9). Pares do mesmo lado do eixo são
// CVD-safe (entrada ΔE 20.8; saída ΔE 6.1 + legenda como encoding secundário);
// entre lados, a posição acima/abaixo do eixo desambigua.
export const COLOR_V4_IN = '#00a870' // verde
export const COLOR_V6_IN = '#3987e5' // azul
export const COLOR_V4_OUT = '#e5484d' // vermelho
export const COLOR_V6_OUT = '#a16207' // amarelo-ouro (amarelo vivo estoura a banda no dark)

export type FamFilter = 'all' | 'v4' | 'v6'

export interface SeriesStats {
  min: number
  max: number
  avg: number
  p95: number
}

/** min/máx/méd/p95 de uma série em Mbps (para a barra sob o gráfico) */
export function seriesStats(values: number[]): SeriesStats {
  if (values.length === 0) return { min: 0, max: 0, avg: 0, p95: 0 }
  const sorted = [...values].sort((a, b) => a - b)
  const sum = sorted.reduce((a, b) => a + b, 0)
  return {
    min: sorted[0],
    max: sorted[sorted.length - 1],
    avg: sum / sorted.length,
    p95: sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))],
  }
}

/** Datasets do espelho empilhado por família (entrada +, saída −) */
export function stackedMirrorDatasets(
  fam: FamFilter,
  v4in: number[],
  v6in: number[],
  v4out: number[],
  v6out: number[],
) {
  const area = (label: string, color: string, data: number[], stack: string, first: boolean) => ({
    label,
    borderColor: color,
    backgroundColor: withAlpha(color, '73'),
    borderWidth: 0,
    data,
    tension: 0,
    stack,
    fill: first ? 'origin' : ('-1' as any),
    pointRadius: 0,
    pointHoverRadius: 4,
  })
  if (fam === 'v4') {
    return [
      area('Entrada', COLOR_IN, v4in, 'in', true),
      area('Saída', COLOR_OUT, v4out.map((v) => -v), 'out', true),
    ]
  }
  if (fam === 'v6') {
    return [
      area('Entrada', COLOR_IN, v6in, 'in', true),
      area('Saída', COLOR_OUT, v6out.map((v) => -v), 'out', true),
    ]
  }
  return [
    area('IPv4 In', COLOR_V4_IN, v4in, 'in', true),
    area('IPv6 In', COLOR_V6_IN, v6in, 'in', false),
    area('IPv4 Out', COLOR_V4_OUT, v4out.map((v) => -v), 'out', true),
    area('IPv6 Out', COLOR_V6_OUT, v6out.map((v) => -v), 'out', false),
  ]
}

/** Alpha em hex de 2 dígitos (ex.: '26' ≈ 15%) sobre uma cor #rrggbb */
export function withAlpha(hex: string, alpha2: string): string {
  return hex + alpha2
}

/** Legenda idêntica nos dois gráficos espelhados */
export const mirroredLegend = {
  display: true,
  labels: { color: '#9ca3af', usePointStyle: true, boxHeight: 6 },
}

/** Tooltip de gráfico espelhado: saída é negativa, exibe valor absoluto */
export const mirroredTooltip = {
  callbacks: {
    label: (ctx: any) => ` ${ctx.dataset.label}: ${Math.abs(ctx.parsed.y).toFixed(2)} Mbps`,
  },
}

/** Ticks do eixo Y espelhado: sempre valor absoluto com unidade */
export function mirroredYTicks(color = '#9ca3af') {
  return {
    color,
    callback: (v: any) => Math.abs(v).toFixed(1) + ' Mbps',
  }
}
