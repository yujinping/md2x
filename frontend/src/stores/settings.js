import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const LS_THEME = 'mpe-theme'
const LS_LANG = 'mpe-lang'

export const useSettingsStore = defineStore('settings', () => {
  const theme = ref(localStorage.getItem(LS_THEME) || 'dark')
  const lang = ref(localStorage.getItem(LS_LANG) || 'zh-CN')

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

  return { theme, lang, setTheme, setLang }
})
