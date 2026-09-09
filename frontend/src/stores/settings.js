import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const LS_THEME = 'mpe-theme'
const LS_LANG = 'mpe-lang'
const LS_FULL_WIDTH = 'mpe-full-width'
const LS_VIEW_MODE = 'mpe-view-mode'

export const useSettingsStore = defineStore('settings', () => {
  const theme = ref(localStorage.getItem(LS_THEME) || 'dark')
  const lang = ref(localStorage.getItem(LS_LANG) || 'zh-CN')
  const fullWidth = ref(localStorage.getItem(LS_FULL_WIDTH) === '1')
  // 默认视图：single（单文件，无文件树）/ folder（文件夹树）/ auto（按启动内容自动）
  const viewMode = ref(localStorage.getItem(LS_VIEW_MODE) || 'auto')

  function applyTheme(val) {
    document.documentElement.classList.toggle('light', val === 'light')
  }
  function applyLang(val) {
    // 同步菜单语言
    try { invoke('set_menu_language', { lang: val }) } catch (_) {}
  }

  function setTheme(val) {
    theme.value = val
    localStorage.setItem(LS_THEME, val)
    applyTheme(val)
  }

  function setLang(val) {
    lang.value = val
    localStorage.setItem(LS_LANG, val)
    applyLang(val)
  }

  // 初始化
  applyTheme(theme.value)
  applyLang(lang.value)

  function setFullWidth(val) {
    fullWidth.value = val
    localStorage.setItem(LS_FULL_WIDTH, val ? '1' : '0')
  }

  function setViewMode(val) {
    viewMode.value = val
    localStorage.setItem(LS_VIEW_MODE, val)
  }

  return { theme, lang, fullWidth, viewMode, setTheme, setLang, setFullWidth, setViewMode }
})
