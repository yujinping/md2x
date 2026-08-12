<script setup>
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'
import { ref, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps({
  hasFile: Boolean,
  isPdfView: Boolean,
  fileName: String,
  canGoBack: Boolean,
  canGoForward: Boolean,
})
const emit = defineEmits(['open-file', 'export-pdf', 'save-pdf', 'show-html', 'export-doc', 'nav-back', 'nav-forward', 'toggle-full-width'])
const settings = useSettingsStore()
const exportOpen = ref(false)

function toggleExport() {
  exportOpen.value = !exportOpen.value
}

function onExportClick(format) {
  exportOpen.value = false
  emit('export-doc', format)
}

function onClickOutside(e) {
  if (exportOpen.value && !e.target.closest('.export-menu')) {
    exportOpen.value = false
  }
}

onMounted(() => document.addEventListener('click', onClickOutside))
onBeforeUnmount(() => document.removeEventListener('click', onClickOutside))

function onToggleFullWidth() {
  settings.setFullWidth(!settings.fullWidth)
  emit('toggle-full-width')
}

async function onOpenFile() {
  try {
    const p = await invoke('plugin:dialog|open', {
      options: {
        filters: [{ name: 'Markdown', extensions: ['md'] }],
        multiple: false,
        directory: false,
        title: 'Select Markdown'
      }
    })
    if (p) emit('open-file', p)
  } catch (_) {}
}
</script>

<template>
  <header
    class="flex items-center justify-between h-[52px] px-5 flex-shrink-0 select-none relative transition-colors duration-300"
    :class="[hasFile ? 'border-b-2 border-amber' : 'border-b-2 border-[var(--border)]']"
    :style="{ background: 'var(--surface)' }"
  >
    <div class="flex items-center gap-2.5 min-w-0">
      <!-- Brand: MD2x 字体式 wordmark，x 是"可变的输出格式"变量 -->
      <span class="flex items-center gap-px flex-shrink-0" :title="t('brandHint', settings.lang)">
        <span class="text-[15px] font-extrabold tracking-tight" :style="{ color: 'var(--text)' }">MD</span>
        <span class="text-[12px] font-bold tracking-tight" :style="{ color: 'var(--text-muted)' }">2</span>
        <span class="text-[16px] font-extrabold italic leading-none transition-colors duration-300" :style="{ color: hasFile ? 'var(--amber)' : 'var(--accent)' }">x</span>
      </span>

      <!-- File name -->
      <span
        v-if="fileName"
        class="font-mono text-xs px-2 py-0.5 rounded max-w-[360px] truncate"
        :style="{ color: fileName ? 'var(--text)' : 'var(--text-muted)', background: 'var(--surface-hover)' }"
      >{{ fileName }}</span>

      <!-- 前进/后退导航 -->
      <div class="flex items-center gap-1 ml-1">
        <button class="btn btn-icon" :disabled="!canGoBack" :title="t('navBack', settings.lang)" @click="emit('nav-back')">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M10 8H4"/><path d="M7 5l-3 3 3 3"/></svg>
        </button>
        <button class="btn btn-icon" :disabled="!canGoForward" :title="t('navForward', settings.lang)" @click="emit('nav-forward')">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M6 8h6"/><path d="M9 5l3 3-3 3"/></svg>
        </button>
      </div>

      <!-- 全宽显示切换 -->
      <button class="btn btn-icon" :class="{ 'btn-active': settings.fullWidth }" :title="t('btnFullWidth', settings.lang)" @click="onToggleFullWidth">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M3 6V3h3"/><path d="M13 6V3h-3"/><path d="M3 10v3h3"/><path d="M13 10v3h-3"/></svg>
      </button>
    </div>

    <div class="flex gap-1.5 flex-shrink-0" style="-webkit-app-region: no-drag">
      <!-- Back to HTML preview -->
      <button v-if="isPdfView" class="btn btn-ghost" @click="emit('show-html')">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M10 3L5 8l5 5"/></svg>
        {{ t('btnBack', settings.lang) }}
      </button>

      <!-- Save PDF -->
      <button v-if="isPdfView" class="btn btn-amber" @click="emit('save-pdf')">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M12 11v1a1 1 0 01-1 1H5a1 1 0 01-1-1v-1"/><path d="M8 3v7"/><path d="M5 7l3 3 3-3"/></svg>
        {{ t('btnSave', settings.lang) }}
      </button>

      <!-- Default actions -->
      <template v-if="!isPdfView">
        <button class="btn" @click="onOpenFile">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M2 5l6-3 6 3v7l-6 3-6-3V5z"/><path d="M2 5l6 3 6-3"/><path d="M8 8v7"/></svg>
          {{ t('btnOpen', settings.lang) }}
        </button>
        <div class="relative export-menu">
          <button class="btn" :disabled="!hasFile" @click.stop="toggleExport">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M12 11v1a1 1 0 01-1 1H5a1 1 0 01-1-1v-1"/><path d="M8 3v7"/><path d="M5 7l3 3 3-3"/></svg>
            {{ t('btnExport', settings.lang) }}
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3"><path d="M4 6l4 4 4-4"/></svg>
          </button>
          <div v-if="exportOpen" class="absolute right-0 top-[36px] z-50 min-w-[140px] rounded-lg border shadow-lg py-1" :style="{ background: 'var(--surface)', borderColor: 'var(--border)' }">
            <button class="dropdown-item" @click="onExportClick('html')">
              <span class="w-2 h-2 rounded-sm" style="background:#e34c26"></span>
              {{ t('exportHtml', settings.lang) }}
            </button>
            <button class="dropdown-item" @click="onExportClick('pdf')">
              <span class="w-2 h-2 rounded-sm" style="background:#e74c3c"></span>
              {{ t('exportPdf', settings.lang) }}
            </button>
            <button class="dropdown-item" @click="onExportClick('docx')">
              <span class="w-2 h-2 rounded-sm" style="background:#2b579a"></span>
              {{ t('exportDocx', settings.lang) }}
            </button>
          </div>
        </div>
      </template>
    </div>
  </header>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: transparent;
  color: var(--text);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}
.btn:hover { background: var(--surface-hover); border-color: var(--border-hover); }
.btn:active { background: var(--surface-active); transform: scale(0.97); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; transform: none; }
.btn-primary { background: var(--accent); border-color: var(--accent); color: #fff; }
.btn-primary:hover { background: var(--accent-hover); border-color: var(--accent-hover); }
.btn-amber { background: var(--amber); border-color: var(--amber); font-weight: 600; }
.btn-amber:hover { background: var(--amber-hover); border-color: var(--amber-hover); }
.btn-ghost { border-color: transparent; color: var(--text-muted); }
.btn-ghost:hover { color: var(--text); }
.btn-icon { width: 32px; padding: 0; justify-content: center; }
.btn-active { background: var(--amber); border-color: var(--amber); color: #fff; }
.btn-active:hover { background: var(--amber-hover); border-color: var(--amber-hover); }
.dropdown-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 14px;
  background: transparent;
  border: none;
  color: var(--text);
  font-size: 12.5px;
  cursor: pointer;
  text-align: left;
  white-space: nowrap;
}
.dropdown-item:hover { background: var(--surface-hover); }
</style>
