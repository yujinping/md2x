import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const LS_THEME = 'mpe-theme'
const LS_LANG = 'mpe-lang'
const LS_FULL_WIDTH = 'mpe-full-width'
const LS_VIEW_MODE = 'mpe-view-mode'
const LS_TEMPLATE_ID = 'mpe-template-id'
const LS_TOOLBAR_VISIBLE = 'mpe-toolbar-visible'
const LS_TREE_VISIBLE = 'mpe-tree-visible'

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
  // HTML 模板 id：default 为内置模板，其余为外置模板目录下的文件名去扩展名。
  // 预览与导出共用同一档，保证「看到的样子 = 导出的样子」。
  const templateId = ref(localStorage.getItem(LS_TEMPLATE_ID) || 'default')
  // 工具栏显隐：默认隐藏（纯阅读场景占多数，把首屏让给正文）；
  // 关掉后只能从「视图 → 显示工具栏」找回，所以必须持久化，
  // 否则重启后又冒出来，用户会以为开关没生效。
  const toolbarVisible = ref(localStorage.getItem(LS_TOOLBAR_VISIBLE) === '1')
  // 文件树显隐偏好：默认显示。只记录「用户主动开/关」的结果，
  // 全宽模式临时收起、打开/关闭文件夹这类流程性动作不写进来，
  // 否则下次启动的初始状态会被上一次的临时状态顶掉
  const treeVisible = ref(localStorage.getItem(LS_TREE_VISIBLE) !== '0')

  // 主题变更订阅者：预览需要据此重新渲染，保证「设置里切的 = 预览里看到的」
  const themeListeners = new Set()

  // 模版变量版本号：值本身不重要，只用作「变量改过了」的信号。
  // 变量的真正存档在后端（预览/PDF/导出都在后端读同一份），
  // 前端只需在保存成功后把版本号 +1，预览据此重新渲染。
  const templateVarsRev = ref(0)

  function bumpTemplateVars() {
    templateVarsRev.value += 1
  }

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

  function setTemplateId(val) {
    if (!val) return
    templateId.value = val
    localStorage.setItem(LS_TEMPLATE_ID, val)
  }

  function setToolbarVisible(val) {
    toolbarVisible.value = !!val
    localStorage.setItem(LS_TOOLBAR_VISIBLE, val ? '1' : '0')
  }

  function setTreeVisible(val) {
    treeVisible.value = !!val
    localStorage.setItem(LS_TREE_VISIBLE, val ? '1' : '0')
  }

  return {
    theme,
    lang,
    fullWidth,
    viewMode,
    templateId,
    templateVarsRev,
    bumpTemplateVars,
    toolbarVisible,
    treeVisible,
    setTheme,
    setLang,
    setFullWidth,
    setViewMode,
    setTemplateId,
    setToolbarVisible,
    setTreeVisible,
    onThemeChange,
    prefersDark
  }
})
