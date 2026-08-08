<script setup>
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from './stores/settings.js'
import { t } from './i18n/index.js'
import Toolbar from './components/Toolbar.vue'
import Welcome from './components/Welcome.vue'
import StatusBar from './components/StatusBar.vue'
import AboutDialog from './components/AboutDialog.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import PdfOverlay from './components/PdfOverlay.vue'

const settings = useSettingsStore()
const previewState = ref(null)
const hasFile = ref(false)
const fileName = ref('')
const statusText = ref('')
const statusType = ref('')
const isPdfView = ref(false)
const isGeneratingPdf = ref(false)

function setStatus(msg, type) {
  statusText.value = t(msg, settings.lang)
  statusType.value = type || ''
}

async function openMdFile(path) {
  previewState.value = null
  setStatus('statusGenerating', 'busy')
  try {
    await invoke('set_file', { path })
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  await loadPreview()
}

async function loadPreview() {
  let tmp
  try {
    tmp = await invoke('get_html')
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  isPdfView.value = false
  hasFile.value = true

  // Read HTML into srcdoc
  let html = ''
  try {
    let off = 0, done = false
    while (!done) {
      const [chunk, last] = await invoke('read_file_chunk', { path: tmp, offset: off })
      html += chunk
      off += 524288
      done = last
    }
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  previewState.value = html

  try {
    const n = await invoke('get_file_name')
    fileName.value = n || ''
  } catch (_) {}

  setStatus('statusPreviewReady', 'ready')
}

async function onExportPdf() {
  if (previewState.value && isPdfView.value) {
    showPdfViewer(previewState.value)
    return
  }

  // 先显示遮罩层，将 PDF 生成推到下一个 macrotask
  // 确保浏览器有时间渲染遮罩层
  isGeneratingPdf.value = true
  await new Promise(resolve => setTimeout(resolve, 50))

  setStatus('statusGeneratingPdf', 'busy')
  try {
    const r = await invoke('preview_pdf')
    previewState.value = r
    showPdfViewer(r)
    setStatus('statusPdfReady', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  } finally {
    isGeneratingPdf.value = false
  }
}

function showPdfViewer(r) {
  isPdfView.value = true
  fileName.value = r.file_name || ''
}

async function onSavePdf() {
  if (!previewState.value || typeof previewState.value === 'string') {
    setStatus('statusNoPdf', 'error')
    return
  }
  try {
    const d = await invoke('plugin:dialog|save', {
      options: {
        filters: [{ name: 'PDF 文档', extensions: ['pdf'] }],
        defaultPath: previewState.value.file_name,
        title: 'Save PDF'
      }
    })
    if (!d) return
    setStatus('statusSaving', 'busy')
    await invoke('save_pdf_as', { src: previewState.value.temp_path, dst: d })
    setStatus('statusPdfSaved', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  }
}

async function onExportDoc(format) {
  const ext = format
  const nameMap = { html: 'HTML 文档', pdf: 'PDF 文档', docx: 'Word 文档' }
  const base = (fileName.value || 'document').replace(/\.md$/i, '')
  try {
    const d = await invoke('plugin:dialog|save', {
      options: {
        filters: [{ name: nameMap[format], extensions: [ext] }],
        defaultPath: base + '.' + ext,
        title: 'Export ' + ext.toUpperCase()
      }
    })
    if (!d) return
    setStatus('statusExporting', 'busy')
    await invoke('export_' + format, { dst: d })
    setStatus('statusExportReady', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  }
}

function showHtmlPreview() {
  isPdfView.value = false
  // 从 PDF 视图返回时重新加载 HTML 预览
  loadPreview()
}

// 文件拖拽监听
async function startDropListener() {
  while (true) {
    try {
      const p = await invoke('wait_for_drop')
      if (p) {
        previewState.value = null
        await openMdFile(p)
      }
    } catch (_) {
      await new Promise(r => setTimeout(r, 100))
    }
  }
}

// File watcher + menu polling
onMounted(() => {
  startDropListener()
  setInterval(async () => {
    try {
      if (await invoke('check_file_changed')) {
        await loadPreview()
      }
    } catch (_) {}
  }, 1500)
  setInterval(async () => {
    try {
      if (await invoke('check_show_about')) showAbout.value = true
      if (await invoke('check_show_settings')) showSettings.value = true
    } catch (_) {}
  }, 500)
  // Initial file check
  invoke('get_file_name').then(n => {
    if (n) { fileName.value = n; loadPreview() }
  }).catch(() => {})
  setStatus('statusReady', 'ready')

  // 键盘快捷键：Cmd+O / Ctrl+O 打开文件
  window.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'o') {
      e.preventDefault()
      onOpenFile()
    }
  })
})

const showAbout = ref(false)
const showSettings = ref(false)
const isDragging = ref(false)

async function onOpenFile() {
  previewState.value = null
  setStatus('statusGenerating', 'busy')
  try {
    const p = await invoke('plugin:dialog|open', {
      options: {
        filters: [{ name: 'Markdown', extensions: ['md'] }],
        multiple: false,
        directory: false,
        title: 'Select Markdown'
      }
    })
    if (!p) return
    await invoke('set_file', { path: p })
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  await loadPreview()
}
</script>

<template>
  <div class="h-screen flex flex-col">
    <Toolbar
      :hasFile="hasFile"
      :isPdfView="isPdfView"
      :fileName="fileName"
      @open-file="onOpenFile"
      @export-pdf="onExportPdf"
      @save-pdf="onSavePdf"
      @show-html="showHtmlPreview"
      @export-doc="onExportDoc"
    />

    <main
      class="flex-1 overflow-hidden bg-white relative transition-colors duration-150"
      :class="{ 'bg-[#f0f0ff]': isDragging }"
      :style="{ outline: isDragging ? '2px dashed #6366f1' : 'none', outlineOffset: isDragging ? '-2px' : '0' }"
      @dragover.prevent="isDragging = true"
      @dragleave="isDragging = false"
      @drop.prevent="isDragging = false"
    >
      <Welcome
        v-if="!hasFile"
        @open-file="onOpenFile"
      />

      <iframe
        v-if="hasFile && !isPdfView && previewState"
        :srcdoc="typeof previewState === 'string' ? previewState : ''"
        class="w-full h-full border-none block"
      />

      <iframe
        v-if="hasFile && isPdfView && previewState && typeof previewState !== 'string'"
        :src="'data:application/pdf;base64,' + previewState.base64"
        class="w-full h-full border-none block"
      />
    </main>

    <StatusBar :text="statusText" :type="statusType" />

    <!-- PDF 生成中的遮罩层 -->
    <PdfOverlay v-if="isGeneratingPdf" :text="t('pdfOverlayPreparing', settings.lang)" />

    <Transition name="fade">
      <AboutDialog v-if="showAbout" @close="showAbout = false" />
    </Transition>

    <Transition name="fade">
      <SettingsDialog v-if="showSettings" @close="showSettings = false" />
    </Transition>
  </div>
</template>
