import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const LS_THEME = 'mpe-theme'
const LS_LANG = 'mpe-lang'
const LS_FULL_WIDTH = 'mpe-full-width'
const LS_VIEW_MODE = 'mpe-view-mode'

// 合法档位：浅色 / 暗色。
// 与导出页面右下角切换按钮保持一致 —— 那边是二态互切，没有「跟随系统」档。
const THEME_VALUES = ['light', 'dark']

export const useSettingsStore = defineStore('settings', () => {
  // 存量配置里可能有历史遗留的 'auto'（旧版三档遗留），按当前系统偏好落定为具体档位
  const savedTheme = localStorage.getItem(LS_THEME)
  const initialTheme =
    savedTheme === 'light' || savedTheme === 'dark'
      ? savedTheme
      : prefersDark()
        ? 'dark'
        : 'light'
  const theme = ref(initialTheme)
  const lang = ref(localStorage.getItem(LS_LANG) || 'zh-CN')
  const fullWidth = ref(localStorage.getItem(LS_FULL_WIDTH) === '1')
  // 默认视图：single（单文件，无文件树）/ folder（文件夹树）/ auto（按启动内容自动）
  const viewMode = ref(localStorage.getItem(LS_VIEW_MODE) || 'auto')

  // 主题变更订阅者：预览需要据此重新渲染，保证「设置里切的 = 预览里看到的」
  const themeListeners = new Set()

  function applyTheme(val) {
    document.documentElement.classList.toggle('light', val === 'light')
  }

  function prefersDark() {
    return !!(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches)
  }

  function applyLang(val) {
    // 同步菜单语言
    try { invoke('set_menu_language', { lang: val }) } catch (_) {}
  }

  function setTheme(val) {
    if (!THEME_VALUES.includes(val)) return
    theme.value = val
    localStorage.setItem(LS_THEME, val)
    applyTheme(val)
    // 通知预览重新渲染：后端渲染 HTML 时会把该档位固化进去
    themeListeners.forEach((fn) => {
      try { fn(val) } catch (_) {}
    })
  }

  /**
   * 订阅主题变更，返回取消订阅函数。
   * 供 App.vue 在主题切换时重新走一遍渲染链路。
   */
  function onThemeChange(fn) {
    themeListeners.add(fn)
    return () => themeListeners.delete(fn)
  }

  function setLang(val) {
    lang.value = val
    localStorage.setItem(LS_LANG, val)
    applyLang(val)
  }

  // 初始化
  applyTheme(theme.value)
  applyLang(lang.value)

  // 旧版可能存了 'auto'，启动时落定为具体档位并回写，避免每次都走迁移分支
  if (localStorage.getItem(LS_THEME) !== theme.value) {
    localStorage.setItem(LS_THEME, theme.value)
  }

  function setFullWidth(val) {
    fullWidth.value = val
    localStorage.setItem(LS_FULL_WIDTH, val ? '1' : '0')
  }

  function setViewMode(val) {
    viewMode.value = val
    localStorage.setItem(LS_VIEW_MODE, val)
  }

  return {
    theme,
    lang,
    fullWidth,
    viewMode,
    setTheme,
    setLang,
    setFullWidth,
    setViewMode,
    onThemeChange,
    prefersDark
  }
})
