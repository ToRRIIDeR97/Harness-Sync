import { useState } from 'react'
import type { LocalSkill, Mode, Presets, Status, ToolPreset, ToolStatus } from './api'
import HarnessLogo from './HarnessLogo'
import { PlusIcon } from './icons'
import { listNames, modeLabels } from './summary'

const modes: Mode[] = ['shared', 'append', 'custom', 'off']
const dragType = 'application/x-harness-skill'

type SkillInfo = { name: string; description: string; files: number }

/** Every skill the editor can pick: the sync file's library plus skills found in this computer's tools. */
export function skillCatalog(status: Status) {
  const catalog = new Map<string, SkillInfo & { tools: string[] }>()
  for (const skill of status.skills.library) catalog.set(skill.name, { ...skill, tools: [] })
  for (const group of status.skills.tools) {
    for (const skill of group.skills) {
      if (skill.state === 'error' && !skill.files) continue
      const entry = catalog.get(skill.name) ?? { name: skill.name, description: skill.description, files: skill.files, tools: [] }
      if (skill.state === 'local' || skill.state === 'inSync') entry.tools.push(group.tool)
      catalog.set(skill.name, entry)
    }
  }
  return catalog
}

/** Draft changes for skills. Skills not yet in the sync file are read from `tool` on save. */
export function skillEdits(status: Status, setDraft: (change: (draft: Presets) => Presets) => void) {
  const inLibrary = new Set(status.skills.library.map(skill => skill.name))
  const catalog = skillCatalog(status)
  const withSource = (draft: Presets, name: string, tool?: string) => {
    if (inLibrary.has(name) || draft.skillSources[name]) return draft.skillSources
    const from = tool ?? catalog.get(name)?.tools[0]
    return from ? { ...draft.skillSources, [name]: from } : draft.skillSources
  }
  return {
    share: (name: string, tool?: string) => setDraft(draft => draft.sharedSkills.includes(name) ? draft : {
      ...draft, sharedSkills: [...draft.sharedSkills, name], skillSources: withSource(draft, name, tool),
    }),
    unshare: (name: string) => setDraft(draft => ({ ...draft, sharedSkills: draft.sharedSkills.filter(other => other !== name) })),
    useVersion: (name: string, tool: string) => setDraft(draft => ({ ...draft, skillSources: { ...draft.skillSources, [name]: tool } })),
    toggleExtra: (toolId: string, name: string) => setDraft(draft => {
      const preset = draft.tools[toolId]
      const adding = !preset.skillExtras.includes(name)
      const extras = adding ? [...preset.skillExtras, name] : preset.skillExtras.filter(other => other !== name)
      return {
        ...draft,
        tools: { ...draft.tools, [toolId]: { ...preset, skillExtras: extras } },
        skillSources: adding ? withSource(draft, name, catalog.get(name)?.tools.includes(toolId) ? toolId : undefined) : draft.skillSources,
      }
    }),
  }
}

export type SkillEdits = ReturnType<typeof skillEdits>

const stateBadge: Record<LocalSkill['state'], string | null> = {
  inSync: 'Synced',
  differs: 'Will update',
  updated: 'Updated',
  error: 'Needs attention',
  local: null,
}

export function SkillsView({ status, draft, edits }: { status: Status; draft: Presets; edits: SkillEdits }) {
  const [over, setOver] = useState(false)
  const catalog = skillCatalog(status)
  const toolName = (id: string) => status.tools.find(tool => tool.id === id)?.name ?? id
  const users = status.tools.filter(tool => ['shared', 'append'].includes(draft.tools[tool.id]?.skillMode ?? 'shared')).map(tool => tool.name)
  const drop = (event: React.DragEvent) => {
    event.preventDefault()
    setOver(false)
    try {
      const { name, tool } = JSON.parse(event.dataTransfer.getData(dragType))
      if (typeof name === 'string') edits.share(name, typeof tool === 'string' ? tool : undefined)
    } catch { /* not a skill */ }
  }
  return (
    <>
      <header className="page-header">
        <h1>Skills</h1>
        <p className="lead">Drag skills into the shared list, or choose Share. Each tool's page chooses what that tool gets.</p>
      </header>

      <section className={`drop-zone ${over ? 'over' : ''}`} aria-label="Shared skills"
        onDragOver={event => { if (event.dataTransfer.types.includes(dragType)) { event.preventDefault(); setOver(true) } }}
        onDragLeave={() => setOver(false)} onDrop={drop}>
        <div className="drop-head">
          <h2>Shared skills</h2>
          <span>{users.length ? `Go to ${listNames(users)}` : 'No tool uses them right now'}</span>
        </div>
        {draft.sharedSkills.length
          ? <div className="chip-list">{draft.sharedSkills.map(name => {
            const source = draft.skillSources[name]
            return (
              <span key={name} className="skill-chip">
                <span className="skill-chip-text">
                  <strong>{name}</strong>
                  <span>{source ? `New from ${toolName(source)}` : catalog.get(name)?.description || 'No description'}</span>
                </span>
                <button className="chip-remove" aria-label={`Stop sharing ${name}`} onClick={() => edits.unshare(name)}>×</button>
              </span>
            )
          })}</div>
          : <p className="drop-hint">Drag a skill here to share it with every computer.</p>}
      </section>

      <h2 className="list-heading">Installed on this computer</h2>
      <div className="tool-groups">
        {status.skills.tools.map(group => {
          const mode = draft.tools[group.tool]?.skillMode ?? 'shared'
          return (
            <section key={group.tool} className="tool-group">
              <div className="tool-group-head">
                <HarnessLogo tool={group.tool} size={28} />
                <strong>{toolName(group.tool)}</strong>
                <span className={`pill mode-${mode}`}>{modeLabels[mode]}</span>
              </div>
              <p className="mono folder">{group.folder}</p>
              {mode === 'off' && <p className="setting-note">Harness Sync leaves this folder alone.</p>}
              {group.skills.length
                ? <ul className="skill-rows">{group.skills.map(skill =>
                  <SkillRow key={skill.name} skill={skill} tool={group.tool} toolName={toolName(group.tool)} draft={draft} edits={edits} />)}
                </ul>
                : <p className="drop-hint">No skills in this folder.</p>}
            </section>
          )
        })}
      </div>
    </>
  )
}

function SkillRow({ skill, tool, toolName, draft, edits }: { skill: LocalSkill; tool: string; toolName: string; draft: Presets; edits: SkillEdits }) {
  const shared = draft.sharedSkills.includes(skill.name)
  const uploading = draft.skillSources[skill.name] === tool
  const draggable = skill.files > 0 && !shared
  const badge = skill.editedOutside ? 'Changed here' : skill.wanted ? stateBadge[skill.state] : skill.state === 'error' ? stateBadge.error : null
  return (
    <li className={`skill-row ${draggable ? 'draggable' : ''}`} draggable={draggable}
      onDragStart={event => { event.dataTransfer.setData(dragType, JSON.stringify({ name: skill.name, tool })); event.dataTransfer.effectAllowed = 'copy' }}>
      <span className="skill-row-head">
        <strong>{skill.name}</strong>
        {badge && <span className={`badge state-${skill.editedOutside ? 'differs' : skill.state}`}>{badge}</span>}
        {shared && <span className="badge shared">Shared</span>}
        {uploading && <span className="badge state-updated">Uploads on save</span>}
      </span>
      {skill.description && <span className="clamp subtle">{skill.description}</span>}
      {skill.message && <span className="error-text">{skill.message}</span>}
      {skill.editedOutside && !uploading && <span className="warn-text">The next sync replaces this copy.</span>}
      {((skill.editedOutside && !uploading) || (!shared && skill.files > 0)) &&
        <span className="skill-row-actions">
          {skill.editedOutside && !uploading && <button className="small-button" onClick={() => edits.useVersion(skill.name, tool)}>Use {toolName}'s version</button>}
          {!shared && skill.files > 0 && <button className="small-button" onClick={() => edits.share(skill.name, tool)}><PlusIcon size={16} />Share</button>}
        </span>}
    </li>
  )
}

/** The skills part of a tool's page: which skills this tool gets. */
export function ToolSkills({ tool, status, draft, preset, onMode, edits, onEditShared }: {
  tool: ToolStatus; status: Status; draft: Presets; preset: ToolPreset; onMode: (mode: Mode) => void; edits: SkillEdits; onEditShared: () => void
}) {
  const catalog = skillCatalog(status)
  const candidates = [...catalog.values()]
    .filter(skill => preset.skillMode === 'custom' || !draft.sharedSkills.includes(skill.name))
    .sort((a, b) => a.name.localeCompare(b.name))
  const ownLabel = `${tool.name} only`
  return (
    <section className="tool-skills">
      <h2 className="list-heading">Skills</h2>
      <div className="mode-picker" role="radiogroup" aria-label={`Which skills ${tool.name} gets`}>
        {modes.map(mode => (
          <button key={mode} role="radio" aria-checked={preset.skillMode === mode} className={preset.skillMode === mode ? 'chosen' : ''} onClick={() => onMode(mode)}>
            {modeLabels[mode]}
          </button>
        ))}
      </div>
      {preset.skillMode === 'off'
        ? <div className="empty-panel small"><p>{tool.name}'s skills folder is not changed by Harness Sync.</p></div>
        : <>
          {preset.skillMode !== 'custom' &&
            <div className="skill-box">
              <div className="section-label shared-label">
                <span>Shared</span>
                <button className="text-button" onClick={onEditShared}>Edit shared skills</button>
              </div>
              <p className="skill-names">{draft.sharedSkills.length ? draft.sharedSkills.join(', ') : 'No shared skills yet.'}</p>
            </div>}
          {preset.skillMode !== 'shared' &&
            <div className="skill-box">
              <div className="section-label own-label"><span>{ownLabel}</span></div>
              {candidates.length
                ? <ul className="check-list">{candidates.map(skill =>
                  <li key={skill.name}>
                    <label>
                      <input type="checkbox" checked={preset.skillExtras.includes(skill.name)} onChange={() => edits.toggleExtra(tool.id, skill.name)} />
                      <span className="skill-text"><strong>{skill.name}</strong>{skill.description && <span className="clamp">{skill.description}</span>}</span>
                    </label>
                  </li>)}
                </ul>
                : <p className="drop-hint">No other skills found on this computer.</p>}
            </div>}
        </>}
    </section>
  )
}
