// Decorative beside the visible tool name; bundled locally for offline use.
export default function HarnessLogo({ tool, size = 24 }: { tool: string; size?: number }) {
  return <img src={`/logos/${tool}.${tool === 'claude' ? 'svg' : 'png'}`} alt="" aria-hidden="true" width={size} height={size} className={`logo logo-${tool}`} />
}
