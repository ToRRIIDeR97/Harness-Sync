import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { api, hasNative, type Mode, type Presets, type Status, type ToolPreset, type ToolStatus } from './api'
import HarnessLogo from './HarnessLogo'
import { BackIcon, CloudIcon, ComputerIcon, FolderIcon, GearIcon, LinesIcon, PlusIcon, SyncIcon, ToneIcon } from './icons'
import { listNames, modeLabels, shortTime, summarize } from './summary'

type View = 'shared' | 'settings' | string

const modes: Mode[] = ['shared', 'append', 'custom', 'off']

function presetsFrom(status: Status | null): Presets {
  const tools: Record<string, ToolPreset> = {}
  for (const tool of status?.tools ?? []) tools[tool.id] = status?.document?.tools[tool.id] ?? { mode: 'shared', text: '' }
  return { shared: status?.document?.shared ?? '', tools }
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)

/** Tools whose instructions on this computer can become the shared preset. */
const localSources = (status: Status) => status.tools.filter(tool => tool.exists && tool.lines > 0 && tool.state !== 'error')

export default function App() {
  const [status, setStatus] = useState<Status | null>(null)
  const [draft, setDraft] = useState<Presets>(presetsFrom(null))
  const [baseRevision, setBaseRevision] = useState<number | null>(null)
  const [view, setView] = useState<View>('shared')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<{ tone: 'ok' | 'error'; text: string } | null>(null)

  const saved = useMemo(() => presetsFrom(status), [status])
  const dirty = status?.document != null && !same(draft, saved)
  const remoteChanged = dirty && baseRevision != null && status?.document?.revision !== baseRevision

  // Adopt fresh status, keeping unsaved edits when there are any.
  const statusRef = useRef<Status | null>(null)
  const draftRef = useRef(draft)
  draftRef.current = draft
  const adopt = useCallback((next: Status, force = false) => {
    const clean = force || same(draftRef.current, presetsFrom(statusRef.current))
    statusRef.current = next
    setStatus(next)
    if (clean) {
      setDraft(presetsFrom(next))
      setBaseRevision(next.document?.revision ?? null)
    }
  }, [])

  const run = useCallback(async (work: () => Promise<Status | null | void>, force = false, success?: string) => {
    setBusy(true)
    setMessage(null)
    try {
      const next = await work()
      if (next) {
        adopt(next, force)
        if (success) setMessage({ tone: 'ok', text: success })
      }
    } catch (failure) {
      setMessage({ tone: 'error', text: String(failure) })
    } finally {
      setBusy(false)
    }
  }, [adopt])

  useEffect(() => {
    if (!hasNative()) return
    run(api.status, true)
    const unlisten = api.onStatusChanged(() => api.status().then(next => adopt(next)).catch(() => {}))
    return () => { unlisten.then(stop => stop()) }
  }, [run, adopt])

  useEffect(() => {
    if (!message || message.tone !== 'ok') return
    const timer = setTimeout(() => setMessage(null), 4000)
    return () => clearTimeout(timer)
  }, [message])

  if (!hasNative()) {
    return <div className="center-screen"><div className="first-run"><h1>Harness Sync</h1><p className="lead">Open the desktop app to manage your instructions.</p></div></div>
  }
  if (!status) return <div className="center-screen"><p className="lead">{message?.text ?? 'Loading…'}</p></div>

  if (status.pending) {
    return <ChooseSource status={status} busy={busy} message={message}
      onJoin={seed => run(() => api.joinSyncFile(seed), true, seed ? 'This computer is now the main one' : 'Connected')}
      onCancel={() => run(api.cancelJoin, true)} />
  }

  if (!status.syncFile) {
    return <FirstRun status={status} busy={busy} message={message}
      onCreate={seed => run(() => api.createSyncFile(seed), true, 'Sync file created')}
      onOpen={() => run(api.openSyncFile, true)} />
  }

  const tool = status.tools.find(item => item.id === view)
  const setTool = (id: string, change: Partial<ToolPreset>) =>
    setDraft(current => ({ ...current, tools: { ...current.tools, [id]: { ...current.tools[id], ...change } } }))
  const save = () => run(() => api.savePresets(draft, baseRevision ?? status.document!.revision), true, 'Saved to all computers')
  const discard = () => { setDraft(saved); setBaseRevision(status.document?.revision ?? null) }

  return (
    <div className="app">
      <Sidebar status={status} draft={draft} saved={saved} view={view} onView={setView} />
      <main className="main">
        {message && <p className={`toast ${message.tone}`} role={message.tone === 'error' ? 'alert' : 'status'}>{message.text}</p>}
        {status.error && <p className="banner error">{status.error}</p>}
        {view === 'settings'
          ? <Settings status={status} busy={busy} dirty={dirty} run={run} />
          : !status.document
            ? <p className="lead">Waiting for the sync file…</p>
            : tool
              ? <ToolView tool={tool} draft={draft} onMode={mode => setTool(tool.id, { mode })} onText={text => setTool(tool.id, { text })} onEditShared={() => setView('shared')} />
              : <SharedView status={status} draft={draft} onChange={shared => setDraft({ ...draft, shared })} />}
        {view !== 'settings' && status.document &&
          <SaveBar dirty={dirty} busy={busy} remoteBy={remoteChanged ? status.document.updatedBy : null} onSave={save} onDiscard={discard} />}
      </main>
    </div>
  )
}

function Sidebar({ status, draft, saved, view, onView }: { status: Status; draft: Presets; saved: Presets; view: View; onView: (view: View) => void }) {
  const summary = summarize(status)
  return (
    <nav className="sidebar" aria-label="Instructions">
      <button className={`status-card tone-${summary.tone}`} onClick={() => onView('settings')}>
        <span className="status-icon"><ToneIcon tone={summary.tone} /></span>
        <span className="status-text"><strong>{summary.title}</strong><span>{summary.detail}</span></span>
      </button>

      <button className={`nav-item primary-item ${view === 'shared' ? 'selected' : ''}`} aria-current={view === 'shared' ? 'page' : undefined} onClick={() => onView('shared')}>
        <LinesIcon size={26} />
        <span className="nav-name">Shared preset</span>
        {draft.shared !== saved.shared && <span className="dot" aria-label="Unsaved" />}
      </button>

      <span className="nav-heading">Tools</span>
      {status.tools.map(tool => {
        const mode = draft.tools[tool.id]?.mode ?? 'shared'
        const changed = !same(draft.tools[tool.id], saved.tools[tool.id])
        return (
          <button key={tool.id} className={`nav-item ${view === tool.id ? 'selected' : ''} ${tool.installed ? '' : 'faded'}`} aria-current={view === tool.id ? 'page' : undefined} onClick={() => onView(tool.id)}>
            <HarnessLogo tool={tool.id} size={30} />
            <span className="nav-name">{tool.name}</span>
            {changed && <span className="dot" aria-label="Unsaved" />}
            <span className={`pill mode-${tool.installed ? mode : 'missing'}`}>{tool.installed ? modeLabels[mode] : 'Not installed'}</span>
          </button>
        )
      })}

      <span className="spacer" />
      <button className={`nav-item ${view === 'settings' ? 'selected' : ''}`} aria-current={view === 'settings' ? 'page' : undefined} onClick={() => onView('settings')}>
        <GearIcon size={24} />
        <span className="nav-name">Settings</span>
      </button>
    </nav>
  )
}

function SharedView({ status, draft, onChange }: { status: Status; draft: Presets; onChange: (text: string) => void }) {
  const users = status.tools.filter(tool => ['shared', 'append'].includes(draft.tools[tool.id]?.mode ?? 'shared')).map(tool => tool.name)
  return (
    <>
      <header className="page-header">
        <h1>Shared preset</h1>
        <p className="lead">{users.length ? `Goes to ${listNames(users)}.` : 'No tool uses it right now.'}</p>
      </header>
      <textarea className="editor" aria-label="Shared preset" spellCheck={false} value={draft.shared}
        placeholder="Write the instructions all your AI tools should follow." onChange={event => onChange(event.target.value)} />
    </>
  )
}

const stateText: Record<ToolStatus['state'], string> = {
  detected: '',
  inSync: 'Up to date on this computer',
  differs: 'Will update on the next sync',
  updated: 'Updated on this computer',
  skipped: 'Preset is empty, file left as is',
  off: 'Harness Sync leaves this file alone',
  notInstalled: 'Not installed on this computer',
  error: 'Needs attention',
}

function ToolView({ tool, draft, onMode, onText, onEditShared }: { tool: ToolStatus; draft: Presets; onMode: (mode: Mode) => void; onText: (text: string) => void; onEditShared: () => void }) {
  const preset = draft.tools[tool.id] ?? { mode: 'shared', text: '' }
  const ownLabel = `${tool.name} only`
  return (
    <>
      <header className="page-header">
        <h1><HarnessLogo tool={tool.id} size={34} />{tool.name} gets</h1>
        <p className={`lead state-${tool.state}`}>{tool.state === 'error' ? tool.message : stateText[tool.state]}</p>
      </header>

      {tool.editedOutside && <p className="banner warn">This file was changed outside Harness Sync. The next sync replaces it.</p>}
      {!tool.installed && <p className="banner info">Your choice still applies on computers where {tool.name} is installed.</p>}

      <div className="mode-picker" role="radiogroup" aria-label={`What ${tool.name} gets`}>
        {modes.map(mode => (
          <button key={mode} role="radio" aria-checked={preset.mode === mode} className={preset.mode === mode ? 'chosen' : ''} onClick={() => onMode(mode)}>
            {modeLabels[mode]}
          </button>
        ))}
      </div>

      {preset.mode === 'off'
        ? <div className="empty-panel"><p>{tool.name}'s instructions file is not changed by Harness Sync.</p></div>
        : <div className="composed">
          {preset.mode !== 'custom' && <>
            <div className="section-label shared-label">
              <span>Shared</span>
              <button className="text-button" onClick={onEditShared}>Edit shared preset</button>
            </div>
            <pre className="shared-preview" aria-label="Shared preset">{draft.shared || 'The shared preset is empty.'}</pre>
          </>}
          {preset.mode !== 'shared' && <>
            <div className="section-label own-label"><span>{ownLabel}</span></div>
            <textarea className="editor own-editor" aria-label={ownLabel} spellCheck={false} value={preset.text}
              placeholder={preset.mode === 'append' ? `Extra lines just for ${tool.name}` : `Instructions just for ${tool.name}`}
              onChange={event => onText(event.target.value)} />
          </>}
        </div>}
    </>
  )
}

function SaveBar({ dirty, busy, remoteBy, onSave, onDiscard }: { dirty: boolean; busy: boolean; remoteBy: string | null; onSave: () => void; onDiscard: () => void }) {
  return (
    <div className="save-bar">
      {remoteBy
        ? <span className="save-state warn"><span className="dot" />{remoteBy} saved a newer version</span>
        : dirty
          ? <span className="save-state warn"><span className="dot" />Unsaved changes</span>
          : <span className="save-state">All changes saved</span>}
      <span className="spacer" />
      <button className="big secondary" disabled={!dirty || busy} onClick={onDiscard}>{remoteBy ? 'Load their version' : 'Discard'}</button>
      <button className="big primary" disabled={!dirty || busy || remoteBy != null} onClick={onSave}>Save to all computers</button>
    </div>
  )
}

function Settings({ status, busy, dirty, run }: { status: Status; busy: boolean; dirty: boolean; run: (work: () => Promise<Status | null | void>, force?: boolean, success?: string) => Promise<void> }) {
  const [name, setName] = useState(status.deviceName)
  const [autostart, setAutostart] = useState<boolean | null>(null)
  const [picking, setPicking] = useState(false)
  const sources = localSources(status)
  const summary = summarize(status)
  useEffect(() => { api.getAutostart().then(setAutostart).catch(() => setAutostart(null)) }, [])
  useEffect(() => setName(status.deviceName), [status.deviceName])
  const conflicts = status.syncFile?.conflictCopies ?? []
  return (
    <>
      <header className="page-header"><h1>Settings</h1></header>
      <section className={`settings-hero tone-${summary.tone}`}>
        <span className="status-icon big"><ToneIcon tone={summary.tone} size={30} /></span>
        <span className="status-text"><strong>{summary.title}</strong><span>{summary.detail}</span></span>
        <span className="spacer" />
        <button className="big primary" disabled={busy} onClick={() => run(api.applyNow, false, 'Synced')}><SyncIcon size={20} />Sync now</button>
      </section>

      {conflicts.length > 0 && <p className="banner warn">Google Drive made {conflicts.length === 1 ? 'a conflict copy' : 'conflict copies'}: {conflicts.join(', ')}. Delete {conflicts.length === 1 ? 'it' : 'them'} once you've checked nothing is missing.</p>}

      <section className="setting">
        <div className="setting-text"><h2>Sync file</h2><p className="mono">{status.syncFile?.path}</p></div>
        <div className="setting-actions">
          <button className="big secondary" disabled={busy} onClick={() => run(api.openSyncFile, true)}><FolderIcon size={20} />Choose another</button>
          <button className="big secondary danger" disabled={busy} onClick={() => run(api.disconnect, true)}>Disconnect</button>
        </div>
      </section>

      {status.document &&
        <section className="setting">
          <div className="setting-text"><h2>Make this computer the main one</h2><p>Your other computers get this computer's instructions.</p></div>
          {!picking && <button className="big secondary" disabled={busy || dirty || !sources.length} onClick={() => setPicking(true)}><ComputerIcon size={20} />Use this computer's</button>}
          {dirty && <p className="setting-note">Save or discard your changes first.</p>}
          {picking &&
            <div className="setting-wide">
              <p className="lead">Which tool has the main instructions? They replace the instructions on all your computers.</p>
              <SeedList tools={sources} busy={busy} onPick={tool => { setPicking(false); run(() => api.useThisComputer(tool!, status.document!.revision), true, 'This computer is now the main one') }} />
              <button className="text-button" onClick={() => setPicking(false)}>Cancel</button>
            </div>}
        </section>}

      <form className="setting" onSubmit={event => { event.preventDefault(); run(() => api.setDeviceName(name), false, 'Name saved') }}>
        <label className="setting-text" htmlFor="device-name"><h2>This computer's name</h2><p>Shown on your other computers when this one saves.</p></label>
        <div className="setting-actions">
          <input id="device-name" className="big-input" value={name} maxLength={64} onChange={event => setName(event.target.value)} />
          <button className="big secondary" type="submit" disabled={busy || !name.trim() || name === status.deviceName}>Save</button>
        </div>
      </form>

      {autostart !== null &&
        <label className="setting toggle-row">
          <span className="setting-text"><h2>Start when I sign in</h2><p>Keeps your tools synced without opening the app.</p></span>
          <input type="checkbox" className="switch" checked={autostart} onChange={event => api.setAutostart(event.target.checked).then(setAutostart)} />
        </label>}
    </>
  )
}

function FirstRun({ status, busy, message, onCreate, onOpen }: { status: Status; busy: boolean; message: { tone: string; text: string } | null; onCreate: (seed: string | null) => void; onOpen: () => void }) {
  const [step, setStep] = useState<'ask' | 'seed'>('ask')
  const sources = localSources(status)
  return (
    <div className="center-screen">
      <div className="first-run">
        {message?.tone === 'error' && <p className="banner error">{message.text}</p>}
        {step === 'ask' ? <>
          <h1>One set of instructions. Every AI tool.</h1>
          <p className="lead">Is this your first computer?</p>
          <div className="choice-grid">
            <button className="choice primary" disabled={busy} onClick={() => setStep('seed')}>
              <PlusIcon size={44} stroke={1.8} />
              <strong>Yes, start here</strong>
              <span>Create the sync file in Google Drive</span>
            </button>
            <button className="choice" disabled={busy} onClick={onOpen}>
              <FolderIcon size={44} stroke={1.8} />
              <strong>No, I set it up already</strong>
              <span>Open the sync file from Google Drive</span>
            </button>
          </div>
        </> : <>
          <button className="text-button back" onClick={() => setStep('ask')}><BackIcon size={18} />Back</button>
          <h1>Start from which instructions?</h1>
          <p className="lead">Your other tools' instructions will be replaced with it.</p>
          <SeedList tools={sources} busy={busy} allowEmpty onPick={onCreate} />
          <p className="hint">Next, save the file in a Google Drive folder.</p>
        </>}

        <div className="found">
          <span className="found-title">Found on this computer</span>
          <div className="found-list">
            {status.tools.filter(tool => tool.installed).map(tool => <span key={tool.id} className="found-chip"><HarnessLogo tool={tool.id} size={24} />{tool.name}</span>)}
            {!status.tools.some(tool => tool.installed) && <span className="found-chip">No supported AI tools yet</span>}
          </div>
        </div>
      </div>
    </div>
  )
}

function SeedList({ tools, busy, allowEmpty = false, onPick }: { tools: ToolStatus[]; busy: boolean; allowEmpty?: boolean; onPick: (tool: string | null) => void }) {
  return (
    <div className="seed-list">
      {tools.map(tool => (
        <button key={tool.id} className="seed" disabled={busy} onClick={() => onPick(tool.id)}>
          <HarnessLogo tool={tool.id} size={32} />
          <strong>{tool.name}</strong>
          <span>{tool.lines} {tool.lines === 1 ? 'line' : 'lines'}</span>
        </button>
      ))}
      {allowEmpty &&
        <button className="seed" disabled={busy} onClick={() => onPick(null)}>
          <PlusIcon size={32} />
          <strong>Start empty</strong>
          <span>Write them in the app</span>
        </button>}
    </div>
  )
}

function ChooseSource({ status, busy, message, onJoin, onCancel }: { status: Status; busy: boolean; message: { tone: string; text: string } | null; onJoin: (seed: string | null) => void; onCancel: () => void }) {
  const [step, setStep] = useState<'choose' | 'seed'>('choose')
  const sources = localSources(status)
  const pending = status.pending!
  return (
    <div className="center-screen">
      <div className="first-run">
        {message?.tone === 'error' && <p className="banner error">{message.text}</p>}
        {step === 'choose' ? <>
          <h1>Which instructions should win?</h1>
          <p className="lead">{pending.name} was last saved by {pending.updatedBy} at {shortTime(pending.updatedAt)}.</p>
          <div className="choice-grid">
            <button className="choice primary" disabled={busy} onClick={() => onJoin(null)}>
              <CloudIcon size={44} stroke={1.8} />
              <strong>Use the synced ones</strong>
              <span>This computer's instructions are replaced</span>
            </button>
            <button className="choice" disabled={busy || !sources.length} onClick={() => setStep('seed')}>
              <ComputerIcon size={44} stroke={1.8} />
              <strong>Use this computer's</strong>
              <span>{sources.length ? 'Your other computers get them' : 'No instructions on this computer yet'}</span>
            </button>
          </div>
          <button className="text-button cancel" disabled={busy} onClick={onCancel}>Cancel</button>
        </> : <>
          <button className="text-button back" onClick={() => setStep('choose')}><BackIcon size={18} />Back</button>
          <h1>Which tool has the main instructions?</h1>
          <p className="lead">They become the shared preset. Tools with different text keep it as their own.</p>
          <SeedList tools={sources} busy={busy} onPick={onJoin} />
          <p className="hint">Your other computers will follow this one.</p>
        </>}
      </div>
    </div>
  )
}
