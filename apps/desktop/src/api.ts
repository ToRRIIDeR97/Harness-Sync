import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type Mode = 'shared' | 'append' | 'custom' | 'off'
export type ToolPreset = { mode: Mode; text: string }
export type ToolState = 'detected' | 'inSync' | 'differs' | 'updated' | 'skipped' | 'off' | 'notInstalled' | 'error'

export type SyncDocument = {
  version: number
  revision: number
  updatedAt: string
  updatedBy: string
  shared: string
  tools: Record<string, ToolPreset>
}

export type ToolStatus = {
  id: string
  name: string
  installed: boolean
  path: string
  exists: boolean
  lines: number
  mode: Mode
  state: ToolState
  editedOutside: boolean
  message: string | null
}

export type Status = {
  deviceName: string
  syncFile: { path: string; name: string; conflictCopies: string[] } | null
  document: SyncDocument | null
  error: string | null
  lastChecked: string | null
  pending: { name: string; updatedBy: string; updatedAt: string } | null
  tools: ToolStatus[]
}

export type Presets = { shared: string; tools: Record<string, ToolPreset> }

export const hasNative = () => '__TAURI_INTERNALS__' in window

export const api = {
  status: () => invoke<Status>('get_status'),
  applyNow: () => invoke<Status>('apply_now'),
  readToolInstructions: (tool: string) => invoke<string>('read_tool_instructions', { tool }),
  createSyncFile: (seedTool: string | null) => invoke<Status | null>('create_sync_file', { seedTool }),
  openSyncFile: () => invoke<Status | null>('open_sync_file'),
  joinSyncFile: (seedTool: string | null) => invoke<Status>('join_sync_file', { seedTool }),
  cancelJoin: () => invoke<Status>('cancel_join'),
  useThisComputer: (seedTool: string, expectedRevision: number) => invoke<Status>('use_this_computer', { seedTool, expectedRevision }),
  disconnect: () => invoke<Status>('disconnect'),
  savePresets: (presets: Presets, expectedRevision: number) => invoke<Status>('save_presets', { presets, expectedRevision }),
  setDeviceName: (name: string) => invoke<Status>('set_device_name', { name }),
  openMain: () => invoke<void>('open_main'),
  getAutostart: () => invoke<boolean>('get_autostart'),
  setAutostart: (enabled: boolean) => invoke<boolean>('set_autostart', { enabled }),
  onStatusChanged: (handler: () => void) => listen('status-changed', handler),
}
