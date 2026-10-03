// Escopo de dispositivo das estatísticas (task 17.9 R-02/R-07):
// `role:<papel>` soma as caixas daquele papel; qualquer outro valor é o IP de um exportador.

export type ExporterRole = 'borda' | 'bng' | 'cgnat'
export type DeviceValue = string

export const ROLES: ExporterRole[] = ['borda', 'bng', 'cgnat']
export const DEFAULT_DEVICE: DeviceValue = 'role:borda'

export const ROLE_LABELS: Record<ExporterRole, string> = {
  borda: 'Borda',
  bng: 'BNG',
  cgnat: 'CGNAT',
}

export function roleLabel(role: string): string {
  return ROLE_LABELS[role as ExporterRole] ?? role
}

export function deviceRole(value: DeviceValue): ExporterRole | null {
  if (!value.startsWith('role:')) return null
  const role = value.slice(5) as ExporterRole
  return ROLES.includes(role) ? role : null
}

export function applyDevice(params: URLSearchParams, value: DeviceValue) {
  params.delete('role')
  params.delete('exporter_ip')
  if (!value) return
  const role = deviceRole(value)
  if (role) params.set('role', role)
  else if (!value.startsWith('role:')) params.set('exporter_ip', value)
}
