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
const LS_EXPORT_PRESETS = 'mpe-export-presets'

/// 预设允许的导出格式：与后端 export_<格式> 命令、菜单项一一对应。
/// 白名单而不是「任意字符串」——存档里混进一个不存在的格式，
/// 界面上会显示成一条正常预设，点了才发现导出没反应
const PRESET_FORMATS = ['html', 'pdf', 'docx', 'folder']

/// 读取预设存档：结构非法或字段越界的整条丢弃，不因一条坏数据拖垮整个列表
function readPresets() {
  let raw
  try {
    raw = JSON.parse(localStorage.getItem(LS_EXPORT_PRESETS) || '[]')
  } catch (_) {
    return []
  }
  if (!Array.isArray(raw)) return []
  return raw
    .filter(p => p && typeof p.id === 'string' && p.id && typeof p.name === 'string' && p.name.trim())
    .filter(p => PRESET_FORMATS.includes(p.format))
    .map(p => ({
      id: p.id,
      name: p.name,
      format: p.format,
      // 模版缺失时落到内置模版：预设不该因为删了模版就整条不可用
      templateId: typeof p.templateId === 'string' && p.templateId ? p.templateId : 'default',
      theme: p.theme === 'dark' ? 'dark' : 'light',
      fullWidth: !!p.fullWidth
    }))
}

// 合法档位：浅色 / 暗色。
// 与导出页面右下角切换按钮保持一致 —— 那边是二态互切，没有「跟随系统」档。
const THEME_VALUES = ['light', 'dark']

// ===== 可渲染类型配置 =====
// 真正存档在后端（<app_data>/render-types.json）：文件树过滤、预览渲染、导出
// 三处都在 Rust 侧读同一份，前端这份只是给设置界面用的镜像。
// 不存 localStorage 的理由与其他设置不同——这一项直接决定「哪些文件能打开」，
// 存两处必然出现「设置里改了、文件树没变」的静默不一致。
const renderTypes = ref([])
const rendererList = ref([])

/// 从后端拉取类型配置与可选渲染器清单（设置界面打开时调用）
async function loadRenderTypes() {
  try {
    const [rules, list] = await Promise.all([
      invoke('get_render_types'),
      invoke('list_renderers'),
    ])
    renderTypes.value = Array.isArray(rules) ? rules : []
    rendererList.value = Array.isArray(list) ? list : []
  } catch (_) {
    // 后端不可用（浏览器调试模式）：留空即可，界面显示「暂无」而不是报错
  }
}

/// 保存类型配置。返回 true 表示后端已接受。
/// 成功后同步刷新本地镜像——后端会做去点/小写归一，不回读就会出现
/// 「我输入 .DBML，界面还显示 .DBML，实际存的是 dbml」的错觉。
async function saveRenderTypes(rules) {
  try {
    await invoke('set_render_types', {
      rules: rules.map(r => ({
        ext: r.ext,
        renderer: r.renderer,
        rendererLabel: r.rendererLabel || '',
        enabled: r.enabled !== false,
      })),
    })
    await loadRenderTypes()
    return true
  } catch (_) {
    return false
  }
}

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

  // 导出预设：把「格式 + 模版 + 主题 + 全宽」存成一组，菜单「文件 → 导出」直达。
  // 模版的可调变量不在预设里——它们按模版 id 存在后端，切模版自然跟着走；
  // 复制多份到预设里反而会出现「同一模版两套配色」的错位。
  const exportPresets = ref(readPresets())

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

  /// 把预设列表同步到原生菜单：菜单项 id 为 `preset:<预设 id>`，
  /// 点击后前端按 id 查表取出完整参数。失败只影响菜单入口，
  /// 设置面板里的列表照常可用，因此不抛错——注意 invoke 返回 Promise，
  /// 必须接 .catch，否则失败会变成一条未处理的 rejection。
  function syncPresetMenu() {
    try {
      invoke('set_export_presets', {
        presets: exportPresets.value.map(p => ({ id: p.id, name: p.name }))
      }).catch(() => {})
    } catch (_) {}
  }

  function persistPresets() {
    localStorage.setItem(LS_EXPORT_PRESETS, JSON.stringify(exportPresets.value))
    syncPresetMenu()
  }

  /// 新增预设：重名直接拒绝（菜单里两行同名无法区分），
  /// 由调用方给出提示，而不是悄悄生成一个看不出差别的条目。
  function addExportPreset(preset) {
    const name = (preset.name || '').trim()
    if (!name) return null
    if (exportPresets.value.some(p => p.name === name)) return null
    const created = {
      id: 'p_' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
      name,
      format: PRESET_FORMATS.includes(preset.format) ? preset.format : 'html',
      templateId: preset.templateId || 'default',
      theme: preset.theme === 'dark' ? 'dark' : 'light',
      fullWidth: !!preset.fullWidth
    }
    exportPresets.value = [...exportPresets.value, created]
    persistPresets()
    return created
  }

  function removeExportPreset(id) {
    exportPresets.value = exportPresets.value.filter(p => p.id !== id)
    persistPresets()
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
    exportPresets,
    setTheme,
    setLang,
    setFullWidth,
    setViewMode,
    setTemplateId,
    setToolbarVisible,
    setTreeVisible,
    syncPresetMenu,
    addExportPreset,
    removeExportPreset,
    onThemeChange,
    prefersDark,
    renderTypes,
    rendererList,
    loadRenderTypes,
    saveRenderTypes,
  }
})
