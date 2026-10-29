<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

// 预览区右键菜单：菜单由父窗口渲染（iframe 内的 DOM 无法溢出到窗口外）。
// 坐标由父窗口传入视口坐标，这里只负责夹到可视区域内。
const props = defineProps({
  x: { type: Number, default: 0 },
  y: { type: Number, default: 0 },
  // 右键时 iframe 内的选中文字；为空表示「复制」不可用
  selection: { type: String, default: '' },
  // 是否打开了文件夹（决定「导出 → 文件夹 → 单个 HTML」是否出现）
  hasFolder: Boolean,
})
const emit = defineEmits(['copy', 'export', 'close'])

const settings = useSettingsStore()
const root = ref(null)
const pos = ref({ left: props.x, top: props.y })
const subOpen = ref(false)
const subFlip = ref(false)
const exportItem = ref(null)

const modifier = (() => {
  const p = (navigator.platform || '').toUpperCase()
  return p.indexOf('MAC') >= 0 ? '⌘' : 'Ctrl'
})()

/// 菜单可能超出右/下边界：渲染后按实际尺寸回调到视口内
onMounted(() => {
  const el = root.value
  if (!el) return
  const r = el.getBoundingClientRect()
  const left = Math.min(props.x, window.innerWidth - r.width - 8)
  const top = Math.min(props.y, window.innerHeight - r.height - 8)
  pos.value = { left: Math.max(8, left), top: Math.max(8, top) }
  document.addEventListener('mousedown', onOutside, true)
})

onBeforeUnmount(() => document.removeEventListener('mousedown', onOutside, true))

function onOutside(e) {
  if (root.value && !root.value.contains(e.target)) emit('close')
}

/// 子菜单默认向右；右侧空间不足时向左展开，避免被窗口裁掉
function onExportHover() {
  subOpen.value = true
  const el = exportItem.value
  if (!el) return
  const r = el.getBoundingClientRect()
  subFlip.value = r.right + 180 > window.innerWidth
}

function doExport(format) {
  emit('export', format)
}
</script>

<template>
  <div
    ref="root"
    class="fixed z-[9997] min-w-[188px] rounded-lg border shadow-lg py-1 text-[12.5px] select-none"
    :style="{
      left: pos.left + 'px',
      top: pos.top + 'px',
      background: 'var(--surface)',
      borderColor: 'var(--border)',
      boxShadow: '0 12px 36px var(--dialog-shadow)',
    }"
    @contextmenu.prevent
  >
    <!-- 复制：选中文字走系统剪贴板（与编辑器菜单同一套实现） -->
    <button
      class="ctx-item"
      :disabled="!selection"
      @click="emit('copy')"
    >
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
        <rect x="5.5" y="5.5" width="8" height="9" rx="1.2" />
        <path d="M10.5 3.5H3.5a1 1 0 0 0-1 1v7" />
      </svg>
      <span class="flex-1 text-left">{{ t('ctxCopy', settings.lang) }}</span>
      <span class="flex items-center gap-0.5 flex-shrink-0">
        <kbd class="ctx-kbd">{{ modifier }}</kbd>
        <kbd class="ctx-kbd">C</kbd>
      </span>
    </button>

    <!-- 导出：与工具栏导出按钮同一组格式 -->
    <div ref="exportItem" class="relative" @mouseenter="onExportHover" @mouseleave="subOpen = false">
      <button class="ctx-item" @click="subOpen = !subOpen">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
          <path d="M12 11v1a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-1" />
          <path d="M8 3v7" />
          <path d="M5 7l3 3 3-3" />
        </svg>
        <span class="flex-1 text-left">{{ t('btnExport', settings.lang) }}</span>
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 flex-shrink-0">
          <path d="M6 4l4 4-4 4" />
        </svg>
      </button>

      <div
        v-if="subOpen"
        class="absolute top-[-6px] min-w-[168px] rounded-lg border shadow-lg py-1"
        :style="{
          background: 'var(--surface)',
          borderColor: 'var(--border)',
          boxShadow: '0 12px 36px var(--dialog-shadow)',
          ...(subFlip ? { right: '100%' } : { left: '100%' }),
        }"
      >
        <button v-if="hasFolder" class="ctx-item" @click="doExport('folder')">
          <span class="w-2 h-2 rounded-sm flex-shrink-0" style="background:#8250df"></span>
          <span class="flex-1 text-left">{{ t('exportFolderHtml', settings.lang) }}</span>
        </button>
        <button class="ctx-item" @click="doExport('html')">
          <span class="w-2 h-2 rounded-sm flex-shrink-0" style="background:#e34c26"></span>
          <span class="flex-1 text-left">{{ t('exportHtml', settings.lang) }}</span>
        </button>
        <button class="ctx-item" @click="doExport('pdf')">
          <span class="w-2 h-2 rounded-sm flex-shrink-0" style="background:#e74c3c"></span>
          <span class="flex-1 text-left">{{ t('exportPdf', settings.lang) }}</span>
        </button>
        <button class="ctx-item" @click="doExport('docx')">
          <span class="w-2 h-2 rounded-sm flex-shrink-0" style="background:#2b579a"></span>
          <span class="flex-1 text-left">{{ t('exportDocx', settings.lang) }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 12px;
  background: transparent;
  border: none;
  color: var(--text);
  font-size: 12.5px;
  cursor: pointer;
  white-space: nowrap;
}
.ctx-item:hover:not(:disabled) { background: var(--surface-hover); }
.ctx-item:disabled { opacity: 0.4; cursor: default; }
.ctx-kbd {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 10.5px;
  background: var(--kbd-bg);
  border: 1px solid var(--border);
  color: var(--text-muted);
}
</style>
