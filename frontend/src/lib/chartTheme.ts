// Tema único dos gráficos de tráfego (ao vivo + histórico).
// Par validado via dataviz validate_palette na surface dark (zinc-900):
// deutan ΔE 10.2, banda de luminosidade OK, contraste ≥ 3:1.
export const COLOR_IN = '#059669' // entrada — verde do sistema (emerald-600)
export const COLOR_OUT = '#d95926' // saída — laranja
export const COLOR_UNKNOWN = '#a1a1aa' // sem direção — neutro deliberado

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
