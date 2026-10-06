import type { Mode, Status } from './api'

export type Tone = 'ok' | 'warn' | 'error' | 'idle'
export type Summary = { tone: Tone; title: string; detail: string }

export const modeLabels: Record<Mode, string> = {
  shared: 'Shared',
  append: 'Shared + extra',
  custom: 'Own',
  off: 'Off',
}

export function shortTime(iso: string | null | undefined) {
  if (!iso) return ''
  const date = new Date(iso)
  const today = new Date().toDateString() === date.toDateString()
  return today
    ? date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
    : date.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

export function listNames(names: string[]) {
  if (names.length <= 1) return names.join('')
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

/** One headline for the whole app: what the user most needs to know. */
export function summarize(status: Status | null): Summary {
  if (!status) return { tone: 'idle', title: 'Loading', detail: '' }
  if (!status.syncFile) return { tone: 'idle', title: 'Not set up', detail: 'Choose a sync file to start' }
  if (status.error) return { tone: 'error', title: "Can't read sync file", detail: status.error }
  const failed = status.tools.filter(tool => tool.state === 'error')
  if (failed.length) return { tone: 'warn', title: 'Needs attention', detail: `${listNames(failed.map(tool => tool.name))} couldn't be updated` }
  if (status.syncFile.conflictCopies.length) return { tone: 'warn', title: 'Check Google Drive', detail: 'Drive made a conflict copy of your sync file' }
  const outside = status.tools.filter(tool => tool.editedOutside)
  if (outside.length) return { tone: 'warn', title: 'Changed outside the app', detail: listNames(outside.map(tool => tool.name)) }
  if (status.tools.some(tool => tool.state === 'differs')) return { tone: 'warn', title: 'Waiting to sync', detail: 'Choose Sync now to update' }
  return { tone: 'ok', title: 'All synced', detail: `Updated ${shortTime(status.document?.updatedAt)}` }
}
