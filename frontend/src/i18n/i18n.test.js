// i18n 完整性测试：中英文键集必须一致。
// 编辑器功能新增了一批文案键，双语靠人肉维护迟早漏一条，
// 这里用键集对比把「英文界面出现裸 key」的问题挡在提交前。
//
// @author yujinping
import { describe, it, expect } from 'vitest'
import { i18n, t } from './index.js'

describe('i18n 完整性', () => {
  it('en 与 zh-CN 的键集完全一致', () => {
    const zh = Object.keys(i18n['zh-CN']).sort()
    const en = Object.keys(i18n['en']).sort()
    const onlyZh = zh.filter((k) => !en.includes(k))
    const onlyEn = en.filter((k) => !zh.includes(k))
    expect(onlyZh, '仅中文有（英文缺）').toEqual([])
    expect(onlyEn, '仅英文有（中文缺）').toEqual([])
  })

  it('编辑器文案键已登记且非空', () => {
    const keys = [
      'editorToggle',
      'editorSaved',
      'editorSaveFail',
      'editorConflict',
      'editorDiscardConfirm',
      'editorCloseConfirm',
      'editorReadFail',
      'editorTab',
      'editorNoFile',
      'editorToggleMac',
      'editorToggleWin',
      'settingsEditorMode',
      'editorModePreview',
      'editorModePreviewDesc',
      'editorModeEdit',
      'editorModeEditDesc',
      'editorModeNote',
      'copyNothingSelected',
      'copied',
      'copyFail',
      'copyNotSupportedPdf',
    ]
    for (const k of keys) {
      expect(i18n['zh-CN'][k], `zh-CN.${k}`).toBeTruthy()
      expect(i18n['en'][k], `en.${k}`).toBeTruthy()
      expect(t(k, 'zh-CN')).not.toBe(k)
      expect(t(k, 'en')).not.toBe(k)
    }
  })

  it('未知 key 原样返回，不抛错', () => {
    expect(t('no_such_key', 'en')).toBe('no_such_key')
  })
})
