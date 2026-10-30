<script setup>
import { ref, computed, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const emit = defineEmits(['close'])
const settings = useSettingsStore()

const platform = ref('')
const defaultBusy = ref(false)
const defaultMsg = ref(null) // { ok: boolean, text: string } | null

// 主题两档：浅色 / 暗色。
// 与导出页面右下角的切换按钮语义一致（那边也只能二态互切）。
// 数据驱动渲染，避免多份重复标记；图标与配色在此集中维护。
const THEME_OPTIONS = [
  {
    value: 'light',
    labelKey: 'themeLight',
    descKey: 'themeLightDesc',
    swatchBg: '#ffffff',
    swatchBorder: '1px solid #d4d6da',
    iconStroke: '#1d1d1f',
    iconColor: '#1d1d1f',
    circles: [[9, 9, 3]],
    paths: [
      'M9 1v2M9 15v2M1 9h2M15 9h2M3.5 3.5l1.5 1.5M13 13l1.5 1.5M3.5 14.5l1.5-1.5M13 5l1.5-1.5'
    ]
  },
  {
    value: 'dark',
    labelKey: 'themeDark',
    descKey: 'themeDarkDesc',
    swatchBg: '#1a1d28',
    swatchBorder: '1px solid #2a2e3e',
    iconFill: 'currentColor',
    iconColor: '#f59e0b',
    fillRule: 'evenodd',
    paths: [
      'M9.528 1.718a.75.75 0 01.162.819A8.97 8.97 0 009 6a9 9 0 009 9 8.97 8.97 0 003.463-.69.75.75 0 01.981.98 10.503 10.503 0 01-9.694 6.46c-5.799 0-10.5-4.701-10.5-10.5 0-4.368 2.667-8.112 6.46-9.694a.75.75 0 01.818.162z'
    ]
  }
]

// ===== 选项卡 =====
// 左侧是分类导航，右侧只渲染当前分类的设置，避免一屏堆满所有选项。
// 分类按「改动频率/心智模型」划分：外观（看一看）、视图（读文件时）、
// 导出（产出时）、图表（写图时）、关联（装环境时）。
const TABS = [
  {
    id: 'appearance',
    labelKey: 'settingsTabAppearance',
    // 太阳：主题/语言这类观感设置
    icon: 'M8 5.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6M8 1v1.6M8 13.4V15M1 8h1.6M13.4 8H15M2.9 2.9l1.15 1.15M11.95 11.95l1.15 1.15M2.9 13.1l1.15-1.15M11.95 4.05l1.15-1.15',
  },
  {
    id: 'view',
    labelKey: 'settingsTabView',
    // 侧栏布局：默认视图设置
    icon: 'M1.8 3.3h12.4v9.4H1.8zM5.8 3.3v9.4',
  },
  {
    id: 'export',
    labelKey: 'settingsTabExport',
    // 导出箭头：模板与产出
    icon: 'M8 1.8v8.2M5 7.2l3 3 3-3M2.6 11.6v1.6a1 1 0 0 0 1 1h8.8a1 1 0 0 0 1-1v-1.6',
  },
  {
    id: 'diagrams',
    labelKey: 'settingsTabDiagrams',
    // 流程图节点：图表渲染能力
    icon: 'M1.8 2.6h5v3.2h-5zM9.2 10.2h5v3.2h-5zM4.3 5.8v3.6a1 1 0 0 0 1 1h3.9',
  },
  {
    id: 'associate',
    labelKey: 'settingsTabAssociate',
    // 链环：文件关联/外部集成
    icon: 'M6.5 9.5a2.4 2.4 0 0 0 3.4 0l1.9-1.9a2.4 2.4 0 0 0-3.4-3.4l-.9.9M9.5 6.5a2.4 2.4 0 0 0-3.4 0L4.2 8.4a2.4 2.4 0 0 0 3.4 3.4l.9-.9',
  },
]
const activeTab = ref('appearance')

// ===== 内置图表渲染能力 =====
// 与后端 mermaid.rs 支持的围栏语言一致（`flowchart`、`sequenceDiagram` 等可直接当代码块语言）。
// 标签直接成对写在这里，不再过 i18n 表——这些是图表类型的专有名词，
// 掺进通用文案表反而不好维护。
const BUILTIN_DIAGRAMS = [
  { lang: 'flowchart', zh: '流程图', en: 'Flowchart' },
  { lang: 'sequenceDiagram', zh: '时序图', en: 'Sequence' },
  { lang: 'classDiagram', zh: '类图', en: 'Class' },
  { lang: 'stateDiagram', zh: '状态图', en: 'State' },
  { lang: 'erDiagram', zh: 'ER 图', en: 'ER diagram' },
  { lang: 'gantt', zh: '甘特图', en: 'Gantt' },
  { lang: 'pie', zh: '饼图', en: 'Pie' },
  { lang: 'journey', zh: '用户旅程', en: 'Journey' },
  { lang: 'gitgraph', zh: 'Git 图', en: 'Git graph' },
  { lang: 'mindmap', zh: '思维导图', en: 'Mindmap' },
  { lang: 'timeline', zh: '时间线', en: 'Timeline' },
  { lang: 'quadrantChart', zh: '象限图', en: 'Quadrant' },
  { lang: 'requirementDiagram', zh: '需求图', en: 'Requirement' },
  { lang: 'block-beta', zh: '块图', en: 'Block' },
  { lang: 'kanban', zh: '看板', en: 'Kanban' },
  { lang: 'architecture', zh: '架构图', en: 'Architecture' },
  { lang: 'C4Context', zh: 'C4 架构图', en: 'C4 model' },
]

onMounted(async () => {
  try {
    platform.value = await invoke('get_platform')
  } catch (_) {}
  loadTemplates()
  loadExtTools()
})

const viewModes = [
  { value: 'single', labelKey: 'viewSingle', descKey: 'viewSingleDesc' },
  { value: 'folder', labelKey: 'viewFolder', descKey: 'viewFolderDesc' },
  { value: 'auto', labelKey: 'viewAuto', descKey: 'viewAutoDesc' },
]
const activeViewMode = computed(
  () => viewModes.find(m => m.value === settings.viewMode) || viewModes[2],
)

// ===== 导出模版选择 =====
// 预览与导出共用所选模版；列表由后端自动扫描模版目录（内置 + 目录包 + 旧式单文件）。
// 模版包约定：<id>/template.html（入口）+ template.json（元数据）+ preview.*（缩略图）。
const templates = ref([])
const templatesError = ref(false)
// 初值为 true：mount 时立刻发起扫描，先置「加载中」可避免闪现一次空目录提示
const templatesLoading = ref(true)

async function loadTemplates() {
  templatesLoading.value = true
  templatesError.value = false
  try {
    templates.value = await invoke('list_templates')
  } catch (_) {
    templatesError.value = true
  } finally {
    templatesLoading.value = false
  }
}

async function openTemplateDir() {
  try { await invoke('open_templates_dir') } catch (_) {}
}

// 复制当前模版为副本：自带模版改坏了不好回退，副本随便改，删了也不影响原模版。
// 只有模版包可复制——内置模版编译在二进制里，旧式单文件模版没有目录可复制。
const copying = ref(false)
const copyHint = ref('')
const copyFailed = ref(false)
const canDuplicate = computed(() => {
  const tpl = templates.value.find(x => x.id === settings.templateId)
  return !!tpl && !tpl.builtin && tpl.packaged
})

async function duplicateSelected() {
  if (!canDuplicate.value || copying.value) return
  copying.value = true
  copyFailed.value = false
  copyHint.value = ''
  try {
    const newId = await invoke('duplicate_template', { id: settings.templateId })
    await loadTemplates()
    copyHint.value = t('templateCopied', settings.lang).replace('{id}', newId)
  } catch (_) {
    copyFailed.value = true
    copyHint.value = t('templateCopyFail', settings.lang)
  } finally {
    copying.value = false
  }
}

// ===== 模版变量 =====
// 变量由模版清单声明（颜色 / 文本），取值存档在后端：
// 预览、PDF、导出、聚合都在后端读同一份，所以这里只管编辑与保存，
// 保存成功后 bump 版本号让预览重新渲染。
const varValues = ref({})
const varSaving = ref(false)
const varError = ref(false)

const activeTpl = computed(() => templates.value.find(t => t.id === settings.templateId))
const varDefs = computed(() => activeTpl.value?.vars || [])
// 有任一项偏离默认值时可「恢复默认」；全默认时按钮置灰，避免无效点击
const hasVarOverrides = computed(() =>
  varDefs.value.some(v => (varValues.value[v.key] ?? v.default) !== v.default)
)

/// 把后端返回的生效值铺进编辑态；保存失败或清单变更后都靠它对齐
function syncVarValues(list) {
  const next = {}
  for (const v of list || []) next[v.key] = v.value
  varValues.value = next
}

// 换模版即换变量集合：必须按新模版的声明重建，否则会带着上一个模版的值
watch(() => settings.templateId, () => {
  varError.value = false
  syncVarValues(varDefs.value)
}, { immediate: true })

// 列表刷新（复制、重新扫描）后同样对齐一次
watch(templates, () => syncVarValues(varDefs.value))

/// 取色器只接受 #rrggbb：保存值可能来自手写 rgb() 或 3 位简写，需归一化
function colorOf(v) {
  const raw = String(varValues.value[v.key] ?? v.default ?? '').trim()
  if (/^#[0-9a-f]{6}$/i.test(raw)) return raw.toLowerCase()
  if (/^#[0-9a-f]{3}$/i.test(raw)) return '#' + raw.slice(1).split('').map(c => c + c).join('').toLowerCase()
  const dflt = String(v.default || '').trim()
  if (/^#[0-9a-f]{6}$/i.test(dflt)) return dflt.toLowerCase()
  if (/^#[0-9a-f]{3}$/i.test(dflt)) return '#' + dflt.slice(1).split('').map(c => c + c).join('').toLowerCase()
  return '#000000'
}

async function onVarChange(v, value) {
  const next = { ...varValues.value, [v.key]: String(value ?? '') }
  varValues.value = next
  await saveVars()
}

/// 保存当前模版的全部变量（整份映射，空值即回默认）
async function saveVars() {
  const tpl = activeTpl.value
  if (!tpl || varSaving.value) return
  varSaving.value = true
  varError.value = false
  try {
    const updated = await invoke('set_template_vars', {
      templateId: tpl.id,
      vars: { ...varValues.value },
    })
    // 用后端归一化后的结果回填：非法值会被丢弃并回落到默认，
    // 界面必须如实反映，否则用户以为改成了却没生效
    const item = templates.value.find(t => t.id === tpl.id)
    if (item) item.vars = updated
    syncVarValues(updated)
    settings.bumpTemplateVars()
  } catch (_) {
    varError.value = true
  } finally {
    varSaving.value = false
  }
}

/// 恢复默认：清空覆盖后交给后端删除存档
async function resetVars() {
  varValues.value = {}
  await saveVars()
}

// 内置模版的名称与描述随界面语言变化（后端只给兜底文案）
function tplName(tpl) {
  return tpl.builtin ? t('templateBuiltinName', settings.lang) : (tpl.name || tpl.id)
}
function tplDesc(tpl) {
  if (tpl.builtin) return t('templateBuiltinDesc', settings.lang)
  return tpl.description || t('templateNoDesc', settings.lang)
}
function tplBadge(tpl) {
  if (tpl.builtin) return t('templateBadgeBuiltin', settings.lang)
  return tpl.packaged ? t('templateBadgeCustom', settings.lang) : t('templateBadgeLegacy', settings.lang)
}
// 卡片底部元信息：版本 / 作者 / 目录名，缺哪个跳过哪个
function tplMeta(tpl) {
  const parts = []
  if (tpl.version) parts.push('v' + tpl.version.replace(/^v/i, ''))
  if (tpl.author) parts.push(tpl.author)
  if (!tpl.builtin && !tpl.packaged) parts.push(tpl.id + '.html')
  return parts.join(' · ')
}

/// 当前选中的 id 在列表里不存在时（如外置模版被删），仍保留用户选择，
/// 后端会回退到内置模版渲染，这里只提示不强制改选
const templateMissing = computed(() =>
  templates.value.length > 0 &&
  !templates.value.some(t => t.id === settings.templateId)
)

// ===== 外部图表工具状态 =====
// d2 / dot 图表块依赖外部 CLI，未装时静默降级保留代码块。
// 这里把检测结果亮出来，让人知道这个功能存在、装了依赖即生效。
const extTools = ref([])

async function loadExtTools() {
  try { extTools.value = await invoke('check_external_tools') } catch (_) {}
}

function onOverlayClick(e) {
  if (e.target === e.currentTarget) emit('close')
}

async function setDefault() {
  if (defaultBusy.value) return
  defaultBusy.value = true
  defaultMsg.value = null
  try {
    await invoke('register_default_md_handler')
    const key =
      platform.value === 'macos'
        ? 'defaultSetOkMac'
        : platform.value === 'windows'
          ? 'defaultSetOkWin'
          : 'defaultSetOkLinux'
    defaultMsg.value = { ok: true, text: t(key, settings.lang) }
  } catch (e) {
    defaultMsg.value = { ok: false, text: t('defaultSetFail', settings.lang) + String(e) }
  } finally {
    defaultBusy.value = false
  }
}
</script>

<template>
  <div
    class="fixed inset-0 z-[9998] flex items-center justify-center"
    :style="{ background: 'var(--overlay-bg-light)', backdropFilter: 'blur(3px)' }"
    @click="onOverlayClick"
  >
    <div
      class="w-[720px] max-w-[94vw] h-[620px] max-h-[92vh] rounded-2xl flex flex-col overflow-hidden"
      :style="{
        background: 'var(--dialog-bg)',
        border: '1px solid var(--dialog-border)',
        boxShadow: '0 24px 80px var(--dialog-shadow)',
      }"
    >
      <!-- Header -->
      <div
        class="flex items-center justify-between px-6 h-14 flex-shrink-0"
        :style="{ borderBottom: '1px solid var(--border)' }"
      >
        <span class="text-base font-bold tracking-tight" :style="{ color: 'var(--text)' }">
          {{ t('settingsTitle', settings.lang) }}
        </span>
        <button
          class="w-8 h-8 rounded-lg border-none flex items-center justify-center cursor-pointer"
          :style="{ background: 'transparent', color: 'var(--text-muted)' }"
          :title="t('btnClose', settings.lang)"
          @click="emit('close')"
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" class="w-4 h-4">
            <path d="M4 4l8 8M12 4l-8 8" />
          </svg>
        </button>
      </div>

      <!-- Body：左侧选项卡 + 右侧设置内容 -->
      <div class="flex flex-1 min-h-0">
        <!-- 左侧选项卡 -->
        <nav
          class="w-[176px] flex-shrink-0 flex flex-col gap-1 p-3"
          :style="{ borderRight: '1px solid var(--border)' }"
        >
          <button
            v-for="tab in TABS"
            :key="tab.id"
            class="flex items-center gap-2.5 w-full px-3 py-2.5 rounded-xl text-[13px] font-medium cursor-pointer transition-all duration-150 border-none text-left"
            :style="{
              background: activeTab === tab.id ? 'rgba(99,102,241,0.10)' : 'transparent',
              color: activeTab === tab.id ? 'var(--accent)' : 'var(--text-muted)',
            }"
            @click="activeTab = tab.id"
          >
            <svg
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linecap="round"
              stroke-linejoin="round"
              class="w-4 h-4 flex-shrink-0"
            >
              <path :d="tab.icon" />
            </svg>
            {{ t(tab.labelKey, settings.lang) }}
          </button>
        </nav>

        <!-- 右侧内容 -->
        <div class="flex-1 min-w-0 overflow-y-auto px-6 py-5">
          <!-- ===== 外观：主题 + 语言 ===== -->
          <template v-if="activeTab === 'appearance'">
            <!-- Theme -->
            <div class="mb-6">
              <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsTheme', settings.lang) }}</div>
              <div class="flex flex-col gap-2.5">
                <button
                  v-for="opt in THEME_OPTIONS"
                  :key="opt.value"
                  class="flex items-center gap-4 w-full px-4 py-3 rounded-xl cursor-pointer transition-all duration-200"
                  :style="{
                    border: '1.5px solid ' + (settings.theme === opt.value ? 'var(--accent)' : 'var(--border)'),
                    background: settings.theme === opt.value ? 'rgba(99,102,241,0.06)' : 'transparent'
                  }"
                  @click="settings.setTheme(opt.value)">
                  <span
                    class="flex items-center justify-center w-10 h-10 rounded-xl flex-shrink-0"
                    :style="{ background: opt.swatchBg, border: opt.swatchBorder }"
                  >
                    <svg
                      viewBox="0 0 24 24"
                      :fill="opt.iconFill || 'none'"
                      :stroke="opt.iconStroke || 'none'"
                      stroke-width="1.6"
                      class="w-5 h-5"
                      :style="{ color: opt.iconColor }"
                    >
                      <path v-for="(d, di) in opt.paths" :key="di" :d="d" :fill-rule="opt.fillRule || 'nonzero'" :clip-rule="opt.fillRule || 'nonzero'" />
                      <circle v-for="(c, ci) in (opt.circles || [])" :key="'c'+ci" :cx="c[0]" :cy="c[1]" :r="c[2]" />
                    </svg>
                  </span>
                  <div class="flex flex-col items-start">
                    <span class="text-sm font-medium" :style="{ color: settings.theme === opt.value ? 'var(--accent)' : 'var(--text)' }">{{ t(opt.labelKey, settings.lang) }}</span>
                    <span class="text-[11px]" :style="{ color: 'var(--text-dim)' }">{{ t(opt.descKey, settings.lang) }}</span>
                  </div>
                </button>
              </div>
            </div>

            <!-- Divider -->
            <div class="w-full h-px mb-6" :style="{ background: 'var(--border)' }"></div>

            <!-- Language -->
            <div class="mb-2">
              <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsLang', settings.lang) }}</div>
              <div class="flex flex-col gap-2.5">
                <button
                  class="flex items-center gap-4 w-full px-4 py-3 rounded-xl cursor-pointer transition-all duration-200"
                  :style="{
                    border: '1.5px solid ' + (settings.lang === 'zh-CN' ? 'var(--accent)' : 'var(--border)'),
                    background: settings.lang === 'zh-CN' ? 'rgba(99,102,241,0.06)' : 'transparent'
                  }"
                  @click="settings.setLang('zh-CN')">
                  <span class="text-xl flex-shrink-0 w-10 h-10 flex items-center justify-center">🇨🇳</span>
                  <div class="flex flex-col items-start">
                    <span class="text-sm font-medium" :style="{ color: settings.lang === 'zh-CN' ? 'var(--accent)' : 'var(--text)' }">中文</span>
                    <span class="text-[11px]" :style="{ color: 'var(--text-dim)' }">简体中文</span>
                  </div>
                </button>
                <button
                  class="flex items-center gap-4 w-full px-4 py-3 rounded-xl cursor-pointer transition-all duration-200"
                  :style="{
                    border: '1.5px solid ' + (settings.lang === 'en' ? 'var(--accent)' : 'var(--border)'),
                    background: settings.lang === 'en' ? 'rgba(99,102,241,0.06)' : 'transparent'
                  }"
                  @click="settings.setLang('en')">
                  <span class="text-xl flex-shrink-0 w-10 h-10 flex items-center justify-center">🇺🇸</span>
                  <div class="flex flex-col items-start">
                    <span class="text-sm font-medium" :style="{ color: settings.lang === 'en' ? 'var(--accent)' : 'var(--text)' }">English</span>
                    <span class="text-[11px]" :style="{ color: 'var(--text-dim)' }">US English</span>
                  </div>
                </button>
              </div>
            </div>
          </template>

          <!-- ===== 视图：默认视图 ===== -->
          <template v-else-if="activeTab === 'view'">
            <div>
              <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsView', settings.lang) }}</div>
              <div class="grid grid-cols-3 gap-2 mb-2.5">
                <button
                  v-for="m in viewModes"
                  :key="m.value"
                  class="px-2 py-2.5 rounded-xl text-[13px] font-medium cursor-pointer transition-all duration-200"
                  :style="{
                    border: '1.5px solid ' + (settings.viewMode === m.value ? 'var(--accent)' : 'var(--border)'),
                    background: settings.viewMode === m.value ? 'rgba(99,102,241,0.08)' : 'transparent',
                    color: settings.viewMode === m.value ? 'var(--accent)' : 'var(--text)',
                  }"
                  @click="settings.setViewMode(m.value)"
                >{{ t(m.labelKey, settings.lang) }}</button>
              </div>
              <p class="text-xs leading-relaxed mb-1.5" :style="{ color: 'var(--text-muted)' }">
                {{ t(activeViewMode.descKey, settings.lang) }}
              </p>
              <p class="text-[11px]" :style="{ color: 'var(--text-dim)' }">{{ t('viewNote', settings.lang) }}</p>
            </div>
          </template>

          <!-- ===== 导出：模版选择（卡片 + 缩略图） ===== -->
          <template v-else-if="activeTab === 'export'">
            <div>
              <div class="flex items-center justify-between gap-2 mb-3">
                <div class="text-xs font-semibold uppercase tracking-wider" :style="{ color: 'var(--text-muted)' }">{{ t('settingsTemplate', settings.lang) }}</div>
                <div class="flex items-center gap-1.5">
                  <!-- 复制选中模版：改自己的副本，原模版保持原样便于对照 -->
                  <button
                    class="h-7 px-2.5 rounded-lg border text-[12px] font-medium flex items-center gap-1.5"
                    :style="{
                      borderColor: 'var(--border)',
                      color: canDuplicate ? 'var(--text)' : 'var(--text-dim)',
                      background: 'transparent',
                      cursor: canDuplicate && !copying ? 'pointer' : 'not-allowed',
                      opacity: canDuplicate && !copying ? 1 : 0.45,
                    }"
                    :title="t('btnDuplicateTemplate', settings.lang)"
                    :disabled="!canDuplicate || copying"
                    @click="duplicateSelected"
                  >
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
                      <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
                      <path d="M10.5 3.5v-1a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h1" />
                    </svg>
                    {{ t('btnDuplicateTemplate', settings.lang) }}
                  </button>
                  <button
                    class="w-7 h-7 rounded-lg border flex items-center justify-center cursor-pointer"
                    :style="{ borderColor: 'var(--border)', color: 'var(--text-muted)', background: 'transparent', opacity: templatesLoading ? 0.5 : 1 }"
                    :title="t('btnRefreshList', settings.lang)"
                    :disabled="templatesLoading"
                    @click="loadTemplates"
                  >
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
                      <path d="M14 8a6 6 0 1 1-1.76-4.24" /><path d="M14 1.5V4h-2.5" />
                    </svg>
                  </button>
                  <button
                    class="h-7 px-2.5 rounded-lg border text-[12px] font-medium cursor-pointer flex items-center gap-1.5"
                    :style="{ borderColor: 'var(--border)', color: 'var(--text)', background: 'transparent' }"
                    :title="t('btnOpenTemplateDir', settings.lang)"
                    @click="openTemplateDir"
                  >
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
                      <path d="M1.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h6a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-10.5a1 1 0 0 1-1-1z" />
                    </svg>
                    {{ t('btnOpenTemplateDir', settings.lang) }}
                  </button>
                </div>
              </div>

              <!-- 模版卡片：点击即选中并持久化（记忆到下次启动） -->
              <div class="grid grid-cols-2 gap-3">
                <button
                  v-for="tpl in templates"
                  :key="tpl.id"
                  class="flex flex-col text-left rounded-xl overflow-hidden cursor-pointer transition-all duration-200"
                  :style="{
                    border: '1.5px solid ' + (settings.templateId === tpl.id ? 'var(--accent)' : 'var(--border)'),
                    background: settings.templateId === tpl.id ? 'rgba(99,102,241,0.06)' : 'transparent',
                  }"
                  :title="tpl.id"
                  @click="settings.setTemplateId(tpl.id)"
                >
                  <!-- 缩略图区：无缩略图时渲染占位图形 -->
                  <span
                    class="relative w-full h-[92px] flex items-center justify-center overflow-hidden"
                    :style="{ background: 'var(--surface)', borderBottom: '1px solid var(--border)' }"
                  >
                    <img
                      v-if="tpl.preview"
                      :src="tpl.preview"
                      :alt="tplName(tpl)"
                      class="w-full h-full object-cover object-top"
                    />
                    <svg
                      v-else
                      viewBox="0 0 64 40"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.4"
                      stroke-linecap="round"
                      class="w-[56px] h-[36px]"
                      :style="{ color: 'var(--text-dim)' }"
                    >
                      <rect x="2" y="2" width="60" height="36" rx="4" />
                      <path d="M10 11h22M10 18h34M10 25h28M10 32h18" />
                    </svg>
                    <!-- 来源标记 -->
                    <span
                      class="absolute top-1.5 left-1.5 text-[10px] px-1.5 py-0.5 rounded-md font-medium"
                      :style="tpl.builtin
                        ? { background: 'rgba(99,102,241,0.14)', color: 'var(--accent)' }
                        : { background: 'rgba(150,150,150,0.18)', color: 'var(--text-muted)' }"
                    >{{ tplBadge(tpl) }}</span>
                    <!-- 选中标记 -->
                    <span
                      v-if="settings.templateId === tpl.id"
                      class="absolute top-1.5 right-1.5 w-[18px] h-[18px] rounded-full flex items-center justify-center"
                      :style="{ background: 'var(--accent)', color: '#fff' }"
                    >
                      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                        <path d="M3.5 8.5l3 3 6-6.5" />
                      </svg>
                    </span>
                  </span>
                  <span class="flex flex-col gap-0.5 px-3 py-2.5 w-full min-w-0">
                    <span
                      class="text-[13px] font-semibold truncate"
                      :style="{ color: settings.templateId === tpl.id ? 'var(--accent)' : 'var(--text)' }"
                    >{{ tplName(tpl) }}</span>
                    <span class="text-[11px] leading-snug line-clamp-2" :style="{ color: 'var(--text-dim)' }">{{ tplDesc(tpl) }}</span>
                    <span v-if="tplMeta(tpl)" class="text-[10px] font-mono truncate" :style="{ color: 'var(--text-dim)' }">{{ tplMeta(tpl) }}</span>
                  </span>
                </button>

                <!-- 空目录：给出明确下一步，而不是一片空白 -->
                <div
                  v-if="!templatesLoading && !templates.length && !templatesError"
                  class="col-span-2 px-3.5 py-6 rounded-xl text-center text-[12px]"
                  :style="{ border: '1.5px dashed var(--border)', color: 'var(--text-dim)' }"
                >{{ t('templateEmpty', settings.lang) }}</div>
              </div>

              <!-- 模版变量：清单里声明了 vars 才出现（内置模版与旧式单文件模版没有） -->
              <div
                v-if="varDefs.length"
                class="mt-4 pt-3.5"
                :style="{ borderTop: '1px solid var(--border)' }"
              >
                <div class="flex items-center justify-between gap-2 mb-2.5">
                  <div class="text-xs font-semibold uppercase tracking-wider" :style="{ color: 'var(--text-muted)' }">
                    {{ t('settingsTemplateVars', settings.lang) }}
                  </div>
                  <button
                    class="h-6 px-2 rounded-lg border text-[11px] font-medium"
                    :style="{
                      borderColor: 'var(--border)',
                      color: hasVarOverrides ? 'var(--text)' : 'var(--text-dim)',
                      background: 'transparent',
                      cursor: hasVarOverrides ? 'pointer' : 'not-allowed',
                      opacity: hasVarOverrides ? 1 : 0.45,
                    }"
                    :disabled="!hasVarOverrides || varSaving"
                    @click="resetVars"
                  >{{ t('btnResetVars', settings.lang) }}</button>
                </div>

                <div class="flex flex-col gap-2.5">
                  <div v-for="v in varDefs" :key="v.key">
                    <div class="flex items-center gap-2">
                      <span
                        class="text-[12px] w-[64px] flex-shrink-0 truncate"
                        :style="{ color: 'var(--text)' }"
                        :title="v.key"
                      >{{ v.label }}</span>
                      <!-- 颜色：取色器 + 当前色值，两者同步 -->
                      <template v-if="v.kind === 'color'">
                        <input
                          type="color"
                          class="w-8 h-7 rounded-md cursor-pointer flex-shrink-0 p-0"
                          :style="{ border: '1px solid var(--border)', background: 'transparent' }"
                          :value="colorOf(v)"
                          @change="onVarChange(v, $event.target.value)"
                        />
                        <code class="text-[11px] font-mono" :style="{ color: 'var(--text-dim)' }">{{ colorOf(v) }}</code>
                      </template>
                      <!-- 文本：字号、字体栈、宽度等自由取值 -->
                      <input
                        v-else
                        type="text"
                        class="flex-1 min-w-0 h-7 px-2.5 rounded-lg text-[12px] font-mono outline-none"
                        :style="{ background: 'var(--surface)', color: 'var(--text)', border: '1px solid var(--border)' }"
                        :value="varValues[v.key] ?? v.default"
                        :placeholder="v.default"
                        @change="onVarChange(v, $event.target.value)"
                      />
                    </div>
                    <p
                      v-if="v.hint"
                      class="mt-1 text-[10.5px] leading-relaxed"
                      :style="{ color: 'var(--text-dim)', paddingLeft: '72px' }"
                    >{{ v.hint }}</p>
                  </div>
                </div>
                <p v-if="varError" class="mt-1.5 text-[11px]" :style="{ color: '#ef4444' }">{{ t('templateVarsFail', settings.lang) }}</p>
              </div>

              <p class="mt-3 text-[11px] leading-relaxed" :style="{ color: 'var(--text-dim)' }">{{ t('templateNote', settings.lang) }}</p>
              <p v-if="copyHint" class="mt-1.5 text-[11px]" :style="{ color: copyFailed ? '#ef4444' : 'var(--accent)' }">{{ copyHint }}</p>
              <p v-if="templatesError" class="mt-1.5 text-[11px]" :style="{ color: '#ef4444' }">{{ t('templateLoadFail', settings.lang) }}</p>
              <p v-else-if="templateMissing" class="mt-1.5 text-[11px]" :style="{ color: 'var(--amber)' }">{{ t('templateMissingNote', settings.lang) }}</p>
            </div>
          </template>

          <!-- ===== 图表：内置渲染能力 + 需外部工具的类型 ===== -->
          <template v-else-if="activeTab === 'diagrams'">
            <!-- 内置：随二进制打包的 Mermaid，离线可用 -->
            <div class="mb-6">
              <div class="flex items-center justify-between mb-3">
                <div class="text-xs font-semibold uppercase tracking-wider" :style="{ color: 'var(--text-muted)' }">{{ t('settingsBuiltinDiagrams', settings.lang) }}</div>
                <span
                  class="text-[10px] px-1.5 py-0.5 rounded-md font-medium font-mono"
                  :style="{ background: 'rgba(99,102,241,0.12)', color: 'var(--accent)' }"
                >Mermaid 11</span>
              </div>
              <div class="grid grid-cols-2 gap-1.5">
                <div
                  v-for="d in BUILTIN_DIAGRAMS"
                  :key="d.lang"
                  class="flex items-center gap-2 px-2.5 py-1.5 rounded-lg min-w-0"
                  :style="{ border: '1px solid var(--border)' }"
                >
                  <span class="w-1.5 h-1.5 rounded-full flex-shrink-0" :style="{ background: 'var(--accent)' }"></span>
                  <span class="text-[12px] flex-shrink-0" :style="{ color: 'var(--text)' }">{{ settings.lang === 'en' ? d.en : d.zh }}</span>
                  <code class="text-[10px] font-mono truncate" :style="{ color: 'var(--text-dim)' }" :title="d.lang">{{ d.lang }}</code>
                </div>
              </div>
              <p class="mt-2.5 text-[11px] leading-relaxed" :style="{ color: 'var(--text-dim)' }">{{ t('settingsBuiltinNote', settings.lang) }}</p>
            </div>

            <!-- Divider -->
            <div class="w-full h-px mb-6" :style="{ background: 'var(--border)' }"></div>

            <!-- 外部工具：未装时静默降级，装了重新导出即生效 -->
            <div>
              <div class="flex items-center justify-between mb-3">
                <div class="text-xs font-semibold uppercase tracking-wider" :style="{ color: 'var(--text-muted)' }">{{ t('settingsExtDiagrams', settings.lang) }}</div>
                <button
                  class="w-7 h-7 rounded-lg border flex items-center justify-center cursor-pointer"
                  :style="{ borderColor: 'var(--border)', color: 'var(--text-muted)', background: 'transparent' }"
                  :title="t('btnRefreshList', settings.lang)"
                  @click="loadExtTools"
                >
                  <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
                    <path d="M14 8a6 6 0 1 1-1.76-4.24" /><path d="M14 1.5V4h-2.5" />
                  </svg>
                </button>
              </div>
              <div class="flex flex-col gap-2">
                <div
                  v-for="tool in extTools"
                  :key="tool.id"
                  class="flex items-center gap-3 px-3.5 py-2.5 rounded-xl"
                  :style="{ border: '1.5px solid var(--border)' }"
                >
                  <span
                    class="w-2 h-2 rounded-full flex-shrink-0"
                    :style="{ background: tool.installed ? 'var(--accent)' : 'var(--text-dim)' }"
                  ></span>
                  <div class="flex flex-col items-start min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="text-[13px] font-semibold font-mono" :style="{ color: 'var(--text)' }">{{ tool.id }}</span>
                      <span
                        class="text-[10px] px-1.5 py-0.5 rounded-md font-medium"
                        :style="tool.installed
                          ? { background: 'rgba(99,102,241,0.12)', color: 'var(--accent)' }
                          : { background: 'rgba(150,150,150,0.15)', color: 'var(--text-dim)' }"
                      >{{ tool.installed ? (tool.version || t('toolInstalled', settings.lang)) : t('toolMissing', settings.lang) }}</span>
                    </div>
                    <span class="text-[11px] leading-snug" :style="{ color: 'var(--text-dim)' }">{{ t('tool_' + tool.id + '_desc', settings.lang) }}</span>
                    <span v-if="!tool.installed" class="text-[11px] font-mono leading-snug" :style="{ color: 'var(--amber)' }">{{ t('tool_' + tool.id + '_install', settings.lang) }}</span>
                    <!-- 已装时显示实际解析到的路径：多版本共存时可据此确认到底是哪个生效 -->
                    <span
                      v-else-if="tool.path"
                      class="text-[11px] font-mono leading-snug truncate max-w-full"
                      :style="{ color: 'var(--text-dim)' }"
                      :title="tool.path"
                    >{{ tool.path }}</span>
                  </div>
                </div>
              </div>
              <p class="mt-2.5 text-[11px] leading-relaxed" :style="{ color: 'var(--text-dim)' }">{{ t('extToolsNote', settings.lang) }}</p>
            </div>
          </template>

          <!-- ===== 关联：默认打开程序 ===== -->
          <template v-else>
            <div>
              <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsDefaultApp', settings.lang) }}</div>
              <p class="text-xs leading-relaxed mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('defaultAppDesc', settings.lang) }}</p>
              <button
                class="h-10 px-5 rounded-xl border-none text-sm font-semibold cursor-pointer tracking-wide transition-all duration-150 hover:scale-[1.02] active:scale-[0.98] flex items-center justify-center gap-2"
                :style="{ background: 'var(--amber)', color: '#fff', opacity: defaultBusy ? 0.6 : 1 }"
                :disabled="defaultBusy"
                @click="setDefault"
              >
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
                  <path d="M8 1.5a6.5 6.5 0 1 0 6.5 6.5" />
                  <path d="M8 5.5V8l1.8 1.8" />
                </svg>
                {{ defaultBusy ? '…' : t('btnSetDefault', settings.lang) }}
              </button>
              <p v-if="defaultMsg" class="mt-2.5 text-xs leading-relaxed" :style="{ color: defaultMsg.ok ? 'var(--accent)' : '#ef4444' }">
                {{ defaultMsg.text }}
              </p>
              <p class="mt-2.5 text-[11px] leading-relaxed" :style="{ color: 'var(--text-dim)' }">{{ t('defaultAppNote', settings.lang) }}</p>
            </div>
          </template>
        </div>
      </div>

      <!-- Footer -->
      <div
        class="flex items-center justify-end px-6 h-14 flex-shrink-0"
        :style="{ borderTop: '1px solid var(--border)' }"
      >
        <button
          class="h-9 px-5 rounded-xl border-none text-sm font-semibold cursor-pointer tracking-wide transition-all duration-150 hover:scale-[1.02] active:scale-[0.98]"
          :style="{ background: 'var(--accent)', color: '#fff' }"
          @click="emit('close')">{{ t('btnClose', settings.lang) }}</button>
      </div>
    </div>
  </div>
</template>
