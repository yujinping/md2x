// 编辑器模式归一化的单测（纯函数，不起 pinia / 不碰 Tauri invoke）。
//
// @author yujinping
import { describe, it, expect } from 'vitest'
import { normalizeEditorMode } from './settings.js'

describe('normalizeEditorMode', () => {
  it('合法档位原样返回', () => {
    expect(normalizeEditorMode('preview')).toBe('preview')
    expect(normalizeEditorMode('edit')).toBe('edit')
  })

  it('非法 / 缺失 / 空值回退默认 preview', () => {
    expect(normalizeEditorMode('auto')).toBe('preview')
    expect(normalizeEditorMode('')).toBe('preview')
    expect(normalizeEditorMode(null)).toBe('preview')
    expect(normalizeEditorMode(undefined)).toBe('preview')
  })

  it('大小写敏感：Edit 不算合法（localStorage 只存小写）', () => {
    expect(normalizeEditorMode('Edit')).toBe('preview')
  })

  it('可指定 fallback（老数据迁移场景）', () => {
    expect(normalizeEditorMode('legacy', 'edit')).toBe('edit')
  })
})
