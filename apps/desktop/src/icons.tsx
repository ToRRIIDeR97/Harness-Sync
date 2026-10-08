import type { Tone } from './summary'

type IconProps = { size?: number; stroke?: number }

const svg = (path: React.ReactNode, { size = 24, stroke = 2 }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={stroke} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{path}</svg>
)

export const CheckIcon = (props: IconProps) => svg(<path d="M20 6 9 17l-5-5" />, { stroke: 2.6, ...props })
export const AlertIcon = (props: IconProps) => svg(<><path d="M12 8v5" /><path d="M12 16.5h.01" /><circle cx="12" cy="12" r="9" /></>, props)
export const LinesIcon = (props: IconProps) => svg(<path d="M4 6h16M4 12h16M4 18h10" />, props)
export const GearIcon = (props: IconProps) => svg(<><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></>, props)
export const PlusIcon = (props: IconProps) => svg(<path d="M12 5v14M5 12h14" />, props)
export const FolderIcon = (props: IconProps) => svg(<path d="M4 7a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" />, props)
export const SyncIcon = (props: IconProps) => svg(<><path d="M20 11a8 8 0 0 0-14.3-4.9L4 8" /><path d="M4 4v4h4" /><path d="M4 13a8 8 0 0 0 14.3 4.9L20 16" /><path d="M20 20v-4h-4" /></>, props)
export const CloudIcon = (props: IconProps) => svg(<path d="M7 18a4.5 4.5 0 0 1-.6-9A6 6 0 0 1 18 8.5a4.5 4.5 0 0 1-.5 9.5z" />, props)
export const ComputerIcon = (props: IconProps) => svg(<><rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 20h8M12 16v4" /></>, props)
export const SkillsIcon = (props: IconProps) => svg(<><path d="M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3z" /><path d="M5 17a3 3 0 0 1 3-3h11" /><path d="M9 8h6" /></>, props)
export const BackIcon = (props: IconProps) => svg(<path d="M15 18l-6-6 6-6" />, props)

export function ToneIcon({ tone, size }: { tone: Tone; size?: number }) {
  return tone === 'ok' ? <CheckIcon size={size} /> : tone === 'idle' ? <SyncIcon size={size} /> : <AlertIcon size={size} />
}
