// Simple v4-like ID generator (no crypto dependency needed)
export function v4Id(prefix?: string): string {
  const hex = () => Math.floor(Math.random() * 16).toString(16);
  const s4 = () => hex() + hex() + hex() + hex();
  const id = `${s4()}${s4()}-${s4()}-4${s4().slice(1)}-${s4()}-${s4()}${s4()}${s4()}`;
  return prefix ? `${prefix}-${id}` : id;
}
