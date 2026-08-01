// Tema único dos gráficos de tráfego (ao vivo + histórico).
// Pares validados via dataviz validate_palette na surface dark (zinc-900):
// in/out deutan ΔE 10.0; v4/v6 deutan ΔE 23+ — todos os checks PASS.
export const COLOR_IN = '#00a870' // entrada — verde vivo
export const COLOR_OUT = '#f05a24' // saída — laranja-avermelhado vivo
export const COLOR_UNKNOWN = '#a1a1aa' // sem direção — neutro deliberado
export const COLOR_V4 = '#00a870' // família IPv4 (modo "por versão")
export const COLOR_V6 = '#8b5cf6' // família IPv6 — violeta

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
