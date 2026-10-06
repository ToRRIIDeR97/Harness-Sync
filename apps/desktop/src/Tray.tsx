import { useCallback, useEffect, useState } from 'react'
import { api, type Status } from './api'
import HarnessLogo from './HarnessLogo'
import { SyncIcon, ToneIcon } from './icons'
import { summarize } from './summary'

/** The small window that opens from the tray icon. */
export default function Tray() {
  const [status, setStatus] = useState<Status | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(() => { api.status().then(setStatus).catch(failure => setError(String(failure))) }, [])
  useEffect(() => {
    refresh()
    const unlisten = api.onStatusChanged(refresh)
    window.addEventListener('focus', refresh)
    return () => { unlisten.then(stop => stop()); window.removeEventListener('focus', refresh) }
  }, [refresh])

  const summary = summarize(status)
  const synced = status?.tools.filter(tool => tool.installed && tool.mode !== 'off') ?? []
  const syncNow = async () => {
    setBusy(true)
    setError(null)
    try { setStatus(await api.applyNow()) } catch (failure) { setError(String(failure)) } finally { setBusy(false) }
  }

  return (
    <main className="tray">
      <div className="tray-status">
        <span className={`tray-icon tone-${summary.tone}`}><ToneIcon tone={summary.tone} size={38} /></span>
        <strong>{summary.title}</strong>
        <span className="tray-detail">{error ?? summary.detail}</span>
      </div>
      {synced.length > 0 &&
        <div className="tray-logos" aria-label="Synced tools">
          {synced.map(tool => <span key={tool.id} title={tool.name}><HarnessLogo tool={tool.id} size={34} /></span>)}
        </div>}
      <span className="spacer" />
      <button className="big primary" onClick={() => api.openMain()}>{status?.syncFile ? 'Edit instructions' : 'Set up'}</button>
      {status?.syncFile && <button className="big secondary" disabled={busy} onClick={syncNow}><SyncIcon size={20} />Sync now</button>}
    </main>
  )
}
