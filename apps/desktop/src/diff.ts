import type { SyncDocument } from './api'

/** The text the next sync writes to a tool's instructions file, or `null` when unmanaged. Mirrors `SyncDocument::render`. */
export function render(document: SyncDocument, id: string): string | null {
  const preset = document.tools[id] ?? {}
  const own = preset.text ?? ''
  let text: string
  switch (preset.mode ?? 'shared') {
    case 'off': return null
    case 'shared': text = document.shared; break
    case 'custom': text = own; break
    default:
      text = !own.trim() ? document.shared
        : !document.shared.trim() ? own
          : `${document.shared.trimEnd()}\n\n${own.trimStart()}`
  }
  return text === '' || text.endsWith('\n') ? text : text + '\n'
}

export type DiffLine = { kind: ' ' | '-' | '+'; text: string }

const lines = (text: string) => text === '' ? [] : text.replace(/\n$/, '').split('\n')

/** A line diff from `before` to `after` using the longest common subsequence. */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = lines(before)
  const b = lines(after)
  const common = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0))
  for (let i = a.length - 1; i >= 0; i--)
    for (let j = b.length - 1; j >= 0; j--)
      common[i][j] = a[i] === b[j] ? common[i + 1][j + 1] + 1 : Math.max(common[i + 1][j], common[i][j + 1])
  const result: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) { result.push({ kind: ' ', text: a[i] }); i++; j++ }
    else if (i < a.length && (j === b.length || common[i + 1][j] >= common[i][j + 1])) result.push({ kind: '-', text: a[i++] })
    else result.push({ kind: '+', text: b[j++] })
  }
  return result
}
